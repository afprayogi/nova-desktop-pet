//! Pembaruan otomatis yang aman. Rilis di GitHub membawa `latest.json` (versi, URL pemasang, SHA-256) yang DITANDATANGANI Ed25519
//! dengan kunci rahasia pengembang; aplikasi hanya menerima manifest yang tanda tangannya cocok dengan kunci publik tertanam
//! (`pembaruan.pub`). Jadi walau akun/rilis GitHub disusupi atau lalu lintas dibelokkan, pemasang palsu tidak akan dipasang.
//! Alur: periksa berkala (tidak memasang tanpa izin) -> Bos setuju -> unduh (dengan bar kemajuan) -> cek SHA-256 -> jalankan
//! pemasang senyap -> NOVA menutup diri, pemasang menggantinya dan menjalankannya lagi. Data pengguna (%APPDATA%\nova) tidak disentuh.
use base64::Engine as _;
use ring::signature::{Ed25519KeyPair, KeyPair, UnparsedPublicKey, ED25519};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Mutex;
use std::time::Duration;

pub const VERSI: &str = env!("CARGO_PKG_VERSION");
const REPO: &str = env!("CARGO_PKG_REPOSITORY");
/// Kunci publik (base64, 32 byte). Kosong = pembaruan belum dikonfigurasi dan dimatikan.
const KUNCI_PUBLIK: &str = include_str!("../pembaruan.pub");
const MAKS_UNDUHAN: u64 = 300 * 1024 * 1024;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Manifest {
    pub versi: String,
    pub url: String,
    pub sha256: String,
    #[serde(default)]
    pub catatan: String,
    pub tanda_tangan: String,
}

/// Pembaruan terverifikasi yang menunggu persetujuan Bos untuk dipasang.
static MENUNGGU: Mutex<Option<Manifest>> = Mutex::new(None);

fn b64() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

/// Pesan yang ditandatangani: versi, URL, dan hash digabung per baris (tanpa kerumitan kanonikalisasi JSON).
fn pesan(versi: &str, url: &str, sha256: &str) -> Vec<u8> {
    format!("{versi}\n{url}\n{sha256}").into_bytes()
}

fn verifikasi(m: &Manifest, kunci_b64: &str) -> bool {
    let (Ok(pk), Ok(sig)) = (b64().decode(kunci_b64.trim()), b64().decode(m.tanda_tangan.trim())) else { return false };
    UnparsedPublicKey::new(&ED25519, pk).verify(&pesan(&m.versi, &m.url, &m.sha256), &sig).is_ok()
}

fn angka(v: &str) -> Vec<u64> {
    v.trim_start_matches('v').split(['.', '-', '+']).map(|p| p.parse().unwrap_or(0)).collect()
}

/// Apakah `baru` lebih tinggi dari `lama` (semver sederhana)? Mencegah downgrade.
pub fn lebih_baru(baru: &str, lama: &str) -> bool {
    let (a, b) = (angka(baru), angka(lama));
    (0..a.len().max(b.len())).map(|i| (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0))).find(|(x, y)| x != y).is_some_and(|(x, y)| x > y)
}

pub fn aktif() -> bool {
    !KUNCI_PUBLIK.trim().is_empty() && !REPO.is_empty()
}

pub fn menunggu() -> Option<Manifest> {
    MENUNGGU.lock().ok().and_then(|m| m.clone())
}

/// Ambil manifest rilis terbaru, verifikasi tanda tangan, dan kembalikan bila versinya lebih baru.
pub async fn periksa(http: &reqwest::Client) -> Result<Option<Manifest>, String> {
    if !aktif() {
        return Err("pembaruan belum dikonfigurasi (kunci publik kosong)".into());
    }
    let url = format!("{REPO}/releases/latest/download/latest.json");
    let m: Manifest = http.get(url).timeout(Duration::from_secs(20)).send().await.and_then(|r| r.error_for_status()).map_err(|e| format!("tidak bisa menghubungi GitHub: {e}"))?.json().await.map_err(|_| "manifest rilis rusak".to_string())?;
    if !verifikasi(&m, KUNCI_PUBLIK) {
        return Err("tanda tangan manifest TIDAK cocok; pembaruan ditolak demi keamanan".into());
    }
    if !m.url.starts_with(&format!("{REPO}/releases/download/")) {
        return Err("URL pemasang di luar repositori resmi; ditolak".into());
    }
    if !lebih_baru(&m.versi, VERSI) {
        return Ok(None);
    }
    if let Ok(mut w) = MENUNGGU.lock() {
        *w = Some(m.clone());
    }
    Ok(Some(m))
}

/// Unduh pemasang (kemajuan dilaporkan), cocokkan SHA-256 yang sudah ditandatangani, lalu jalankan senyap dan keluar.
pub async fn pasang(http: &reqwest::Client) -> String {
    let Some(m) = menunggu() else { return "Belum ada pembaruan terverifikasi. Ketik 'cek pembaruan' dulu.".into() };
    let job = crate::progres::mulai(&format!("Memperbarui NOVA ke {}", m.versi), "mengunduh pemasang");
    match unduh(http, &m, job).await {
        Ok(path) => {
            crate::progres::set(job, 98, "menjalankan pemasang");
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP: pemasang tetap hidup setelah NOVA menutup diri
                let r = std::process::Command::new(&path).args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/CLOSEAPPLICATIONS"]).creation_flags(0x0000_0208).spawn();
                if r.is_err() {
                    crate::progres::gagal(job, "pemasang tidak bisa dijalankan");
                    return "Pemasang sudah diunduh dan terverifikasi tetapi tidak bisa dijalankan. Jalankan manual dari folder sementara.".into();
                }
            }
            crate::progres::selesai(job, "NOVA menutup diri untuk memperbarui");
            tokio::spawn(async {
                tokio::time::sleep(Duration::from_secs(2)).await;
                std::process::exit(0);
            });
            format!("Pembaruan {} terverifikasi. NOVA menutup diri sebentar lalu menyala lagi dengan versi baru.", m.versi)
        }
        Err(e) => {
            crate::progres::gagal(job, "pembaruan gagal");
            format!("Pembaruan gagal: {e}. Versi yang terpasang tetap aman dipakai.")
        }
    }
}

async fn unduh(http: &reqwest::Client, m: &Manifest, job: u64) -> Result<std::path::PathBuf, String> {
    use std::io::Write;
    let dir = std::env::temp_dir().join("NOVA-Update");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("NOVA-Setup-{}.exe", m.versi.replace(|c: char| !c.is_ascii_alphanumeric() && c != '.', "_")));
    let mut r = http.get(&m.url).timeout(Duration::from_secs(900)).send().await.and_then(|r| r.error_for_status()).map_err(|e| format!("unduhan gagal: {e}"))?;
    let total = r.content_length().unwrap_or(0);
    if total > MAKS_UNDUHAN {
        return Err("ukuran pemasang tidak wajar".into());
    }
    let mut f = std::fs::File::create(&path).map_err(|e| e.to_string())?;
    let (mut h, mut got) = (Sha256::new(), 0u64);
    while let Some(ch) = r.chunk().await.map_err(|e| format!("unduhan terputus: {e}"))? {
        got += ch.len() as u64;
        if got > MAKS_UNDUHAN {
            let _ = std::fs::remove_file(&path);
            return Err("unduhan melebihi batas ukuran".into());
        }
        h.update(&ch);
        f.write_all(&ch).map_err(|e| e.to_string())?;
        if total > 0 {
            crate::progres::set(job, (got * 90 / total) as u8, &format!("mengunduh {:.1}/{:.1} MB", got as f64 / 1e6, total as f64 / 1e6));
        }
    }
    drop(f);
    crate::progres::set(job, 95, "memeriksa keaslian (SHA-256)");
    let hex: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if !hex.eq_ignore_ascii_case(m.sha256.trim()) {
        let _ = std::fs::remove_file(&path);
        return Err("hash pemasang tidak cocok dengan manifest bertanda tangan; file dibuang".into());
    }
    Ok(path)
}

/// Pemeriksaan berkala (dipanggil dari pekerjaan latar): hanya memberi tahu, tidak pernah memasang sendiri.
pub async fn cek_berkala(tx: crate::engine::Tx) {
    if !aktif() {
        return;
    }
    let http = crate::research::client(30);
    tokio::time::sleep(Duration::from_secs(90)).await;
    loop {
        if let Ok(Some(m)) = periksa(&http).await {
            let tanda = crate::config::data_dir().join("pembaruan_dilihat.txt");
            if std::fs::read_to_string(&tanda).map(|t| t.trim() != m.versi).unwrap_or(true) {
                let _ = std::fs::write(&tanda, &m.versi);
                crate::notify::toast("NOVA - pembaruan", &format!("Versi {} tersedia (kini {VERSI}).", m.versi));
                tx.send(crate::engine::Event::Ask { question: format!("NOVA versi {} tersedia (kini {VERSI}). {} Perbarui sekarang?", m.versi, crate::research::truncate(&m.catatan, 160)), options: vec!["perbarui sekarang".into(), "tunda pembaruan".into()] });
            }
        }
        tokio::time::sleep(Duration::from_secs(12 * 3600)).await;
    }
}

/// Perintah alat `pembaruan`: versi | cek | pasang | tunda
pub async fn jalankan(http: &reqwest::Client, arg: &str) -> String {
    match arg.trim() {
        "pasang" => pasang(http).await,
        "tunda" => "Oke, kuingatkan lagi nanti. Kapan saja: 'cek pembaruan'.".into(),
        "cek" => match periksa(http).await {
            Ok(Some(m)) => format!("Versi {} tersedia (kini {VERSI}). {}\nKetik 'perbarui sekarang' untuk memasang (diunduh, diverifikasi, lalu dipasang).", m.versi, crate::research::truncate(&m.catatan, 400)),
            Ok(None) => format!("NOVA sudah versi terbaru ({VERSI})."),
            Err(e) => format!("Tidak bisa memeriksa pembaruan: {e}."),
        },
        _ => format!("NOVA versi {VERSI}. Pembaruan otomatis: {}.", if aktif() { "aktif (diverifikasi tanda tangan digital)" } else { "belum dikonfigurasi" }),
    }
}

// ---------- alat pengembang (CLI) ----------

/// `nova --buat-kunci <folder-rahasia>`: kunci privat ke folder itu (JANGAN di repo), kunci publik ke ./pembaruan.pub.
pub fn buat_kunci(folder: &std::path::Path) -> Result<String, String> {
    let rng = ring::rand::SystemRandom::new();
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).map_err(|_| "gagal membuat kunci".to_string())?;
    let pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).map_err(|_| "kunci rusak".to_string())?;
    std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    let priv_path = folder.join("kunci-privat.b64");
    if priv_path.exists() {
        return Err(format!("{} sudah ada; tidak ditimpa (hapus manual bila yakin)", priv_path.display()));
    }
    std::fs::write(&priv_path, b64().encode(pkcs8.as_ref())).map_err(|e| e.to_string())?;
    std::fs::write("pembaruan.pub", b64().encode(pair.public_key().as_ref())).map_err(|e| e.to_string())?;
    Ok(format!("Kunci privat: {}\nKunci publik: pembaruan.pub (commit file ini).\nSimpan kunci privat sebagai secret GitHub NOVA_SIGNING_KEY dan cadangkan; jangan pernah di-commit.", priv_path.display()))
}

/// `nova --rilis <pemasang.exe> <url-unduhan> <berkas-catatan>`: tulis latest.json bertanda tangan di sebelah pemasang.
/// Kunci privat dibaca dari env NOVA_SIGNING_KEY (base64 PKCS#8).
pub fn rilis(pemasang: &std::path::Path, url: &str, catatan_file: &str) -> Result<String, String> {
    let kunci = std::env::var("NOVA_SIGNING_KEY").map_err(|_| "env NOVA_SIGNING_KEY kosong".to_string())?;
    let pkcs8 = b64().decode(kunci.trim()).map_err(|_| "NOVA_SIGNING_KEY bukan base64 yang sah".to_string())?;
    let pair = Ed25519KeyPair::from_pkcs8(&pkcs8).map_err(|_| "NOVA_SIGNING_KEY bukan kunci Ed25519 PKCS#8".to_string())?;
    let data = std::fs::read(pemasang).map_err(|e| format!("tidak bisa membaca pemasang: {e}"))?;
    let sha256: String = Sha256::digest(&data).iter().map(|b| format!("{b:02x}")).collect();
    let sig = b64().encode(pair.sign(&pesan(VERSI, url, &sha256)).as_ref());
    let catatan = std::fs::read_to_string(catatan_file).unwrap_or_default().trim().chars().take(600).collect();
    let m = Manifest { versi: VERSI.into(), url: url.into(), sha256, catatan, tanda_tangan: sig };
    let out = pemasang.with_file_name("latest.json");
    std::fs::write(&out, serde_json::to_string_pretty(&m).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    Ok(format!("{} ditulis (versi {VERSI}, sha256 {}…)", out.display(), &m.sha256[..12]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pasangan() -> (Ed25519KeyPair, String) {
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).unwrap();
        let p = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
        let pubb64 = b64().encode(p.public_key().as_ref());
        (p, pubb64)
    }

    fn manifest(p: &Ed25519KeyPair) -> Manifest {
        let (v, u, h) = ("9.9.9", "https://github.com/x/y/releases/download/v9.9.9/s.exe", "ab12");
        Manifest { versi: v.into(), url: u.into(), sha256: h.into(), catatan: String::new(), tanda_tangan: b64().encode(p.sign(&pesan(v, u, h)).as_ref()) }
    }

    #[test]
    fn tanda_tangan_sah_diterima_dan_pemalsuan_ditolak() {
        let (p, pubkey) = pasangan();
        let m = manifest(&p);
        assert!(verifikasi(&m, &pubkey));
        let mut palsu = m.clone();
        palsu.sha256 = "ffff".into(); // pemasang diganti
        assert!(!verifikasi(&palsu, &pubkey));
        let mut palsu2 = m.clone();
        palsu2.url = "https://evil.example/s.exe".into();
        assert!(!verifikasi(&palsu2, &pubkey));
        let (_, kunci_lain) = pasangan();
        assert!(!verifikasi(&m, &kunci_lain)); // ditandatangani kunci lain
        assert!(!verifikasi(&m, ""));
    }

    /// Server HTTP mini di localhost yang menyajikan `isi` untuk satu permintaan.
    async fn sajikan(isi: Vec<u8>) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let alamat = l.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut c, _)) = l.accept().await {
                let mut buf = [0u8; 1024];
                let _ = c.read(&mut buf).await;
                let _ = c.write_all(format!("HTTP/1.1 200 OK
Content-Length: {}
Connection: close

", isi.len()).as_bytes()).await;
                let _ = c.write_all(&isi).await;
            }
        });
        format!("http://{alamat}/NOVA-Setup.exe")
    }

    #[tokio::test]
    async fn unduhan_diterima_bila_hash_cocok_dan_dibuang_bila_tidak() {
        let isi: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        let hex: String = Sha256::digest(&isi).iter().map(|b| format!("{b:02x}")).collect();
        let http = reqwest::Client::new();
        let (p, _) = pasangan();
        let mut m = manifest(&p);
        m.versi = "9.9.1".into();
        m.url = sajikan(isi.clone()).await;
        m.sha256 = hex;
        let path = unduh(&http, &m, crate::progres::mulai("uji", "x")).await.expect("hash cocok harus diterima");
        assert_eq!(std::fs::read(&path).unwrap(), isi);
        let _ = std::fs::remove_file(&path);

        m.versi = "9.9.2".into();
        m.url = sajikan(isi).await;
        m.sha256 = "00".repeat(32);
        let err = unduh(&http, &m, crate::progres::mulai("uji2", "x")).await.unwrap_err();
        assert!(err.contains("hash"), "{err}");
        assert!(!std::env::temp_dir().join("NOVA-Update").join("NOVA-Setup-9.9.2.exe").exists(), "file palsu harus dibuang");
    }

    #[test]
    fn perbandingan_versi() {
        assert!(lebih_baru("0.3.1", "0.3.0"));
        assert!(lebih_baru("v1.0.0", "0.9.9"));
        assert!(lebih_baru("0.10.0", "0.9.0"));
        assert!(!lebih_baru("0.3.0", "0.3.0"));
        assert!(!lebih_baru("0.2.9", "0.3.0")); // tidak boleh downgrade
    }
}
