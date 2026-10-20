//! Skrip tingkat 2: bila resep dari alat yang ada tidak cukup, model boleh menulis skrip PowerShell/Python kecil. Kodenya SELALU
//! ditampilkan penuh, dipindai pola berbahaya (jaringan, penyusupan sistem, eval/enkode), diberi hash SHA-256, dan baru disimpan
//! setelah Bos setuju. Tiap `jalankan` meminta izin Bos lagi, memeriksa ulang hash + pindaian, lalu jalan di folder kerja sendiri
//! dengan lingkungan dibersihkan (tanpa kunci API) dan batas waktu. Ini BUKAN sandbox sistem operasi: skrip tetap berjalan sebagai
//! akun Bos, jadi pelindung utamanya adalah kode yang terlihat + izin per jalan + pemindai.
use crate::alat;
use crate::config::data_dir;
use crate::progres;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const MAKS_KODE: usize = 4000;
const MAKS_SKRIP: usize = 20;
const BATAS_DETIK: u64 = 60;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Skrip {
    pub nama: String,
    pub bahasa: String,
    pub deskripsi: String,
    pub kode: String,
    pub sha256: String,
    pub dibuat: i64,
}

static DRAFT: Mutex<Option<Skrip>> = Mutex::new(None);

fn file() -> std::path::PathBuf {
    data_dir().join("skrip.json")
}

pub fn daftar() -> Vec<Skrip> {
    std::fs::read_to_string(file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn tulis(v: &[Skrip]) -> bool {
    std::fs::write(file(), serde_json::to_string_pretty(v).unwrap_or_default()).is_ok()
}

pub fn ada_draft() -> bool {
    DRAFT.lock().map(|d| d.is_some()).unwrap_or(false)
}

pub fn hash(kode: &str) -> String {
    Sha256::digest(kode.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

/// Pola yang menolak skrip sepenuhnya: jaringan keluar, eval/enkode, mengubah sistem/registri/tugas, mengunduh-lalu-menjalankan.
const TERLARANG: &[&str] = &[
    "invoke-webrequest", "invoke-restmethod", "iwr ", "irm ", "curl", "wget", "downloadstring", "downloadfile", "webclient", "bitstransfer", "net.sockets", "tcpclient", "socket",
    "urllib", "requests", "http.client", "httpx", "aiohttp", "ftplib", "smtplib", "paramiko",
    "invoke-expression", "iex ", "iex(", "-encodedcommand", "frombase64string", "base64", "eval(", "exec(", "__import__", "compile(", "ctypes", "marshal", "pickle",
    "set-executionpolicy", "new-itemproperty", "set-itemproperty", "reg add", "reg delete", "regedit", "hklm", "hkcu", "schtasks", "register-scheduledtask", "new-service", "sc.exe", "sc create",
    "net user", "net localgroup", "netsh", "bcdedit", "diskpart", "format-volume", "vssadmin", "cipher /w", "start-process -verb runas", "runas", "add-mppreference", "set-mppreference",
    "get-credential", "convertto-securestring", "password", "api_key", "apikey", "token", "keylog", "getasynckeystate", "setwindowshookex", "os.system", "shell=true", "subprocess.popen(\"cmd", "powershell -", "cmd /c", "cmd.exe",
];

/// Pola yang hanya diberi peringatan di pratinjau (perlu mata Bos): menulis/menghapus berkas, proses lain.
const WASPADA: &[&str] = &["remove-item", "del ", "rmdir", "shutil.rmtree", "os.remove", "os.unlink", "set-content", "out-file", "add-content", "move-item", "copy-item", "rename-item", "start-process", "subprocess", "stop-process", "taskkill", "open("];

pub fn pindai(kode: &str) -> Result<Vec<&'static str>, String> {
    let l = kode.to_lowercase();
    if let Some(p) = TERLARANG.iter().find(|p| l.contains(*p)) {
        return Err(format!("mengandung pola terlarang '{}'", p.trim()));
    }
    Ok(WASPADA.iter().copied().filter(|p| l.contains(p)).collect())
}

fn validasi(v: &Value) -> Result<Skrip, String> {
    let nama = v["nama"].as_str().unwrap_or("").trim().to_string();
    if !(3..=30).contains(&nama.chars().count()) || !nama.chars().all(|c| c.is_ascii_alphanumeric() || "-_".contains(c)) {
        return Err("nama skrip 3-30 karakter huruf/angka/-/_ tanpa spasi".into());
    }
    let bahasa = v["bahasa"].as_str().unwrap_or("").to_lowercase();
    if !matches!(bahasa.as_str(), "powershell" | "python") {
        return Err("bahasa harus powershell atau python".into());
    }
    let kode = v["kode"].as_str().unwrap_or("").replace("\r\n", "\n");
    if kode.trim().is_empty() || kode.chars().count() > MAKS_KODE {
        return Err(format!("kode kosong atau lebih dari {MAKS_KODE} karakter"));
    }
    pindai(&kode)?;
    Ok(Skrip { nama, bahasa, deskripsi: v["deskripsi"].as_str().unwrap_or("").chars().take(160).collect(), sha256: hash(&kode), kode, dibuat: chrono::Local::now().timestamp() })
}

fn pratinjau(s: &Skrip) -> String {
    let w = pindai(&s.kode).unwrap_or_default();
    let peringatan = if w.is_empty() { String::new() } else { format!("\n⚠ Perhatikan: kode menyentuh berkas/proses ({}).", w.join(", ")) };
    format!(
        "📜 Rancangan skrip ({}): {}\n{}\nSHA-256: {}…\n```\n{}\n```{peringatan}\nSkrip hanya jalan setelah kamu izinkan SETIAP kali, di folder kerja sendiri dengan batas {BATAS_DETIK} detik, tanpa jaringan/registri/tugas terjadwal (dipindai). Simpan atau batalkan di bawah.",
        s.bahasa,
        s.nama,
        s.deskripsi,
        &s.sha256[..12],
        s.kode
    )
}

pub async fn buat(http: &reqwest::Client, permintaan: &str) -> String {
    let permintaan = permintaan.trim();
    if permintaan.chars().count() < 8 {
        return "Jelaskan apa yang harus dikerjakan skrip, mis. 'buat skrip python hitung jumlah baris semua file .rs di folder ini'.".into();
    }
    if daftar().len() >= MAKS_SKRIP {
        return format!("Sudah ada {MAKS_SKRIP} skrip (batas). Hapus yang tak dipakai dulu.");
    }
    let job = progres::mulai(&format!("Skrip: {}", crate::research::truncate(permintaan, 40)), "memahami permintaan");
    let sys = "Kamu penulis skrip kecil untuk asisten NOVA di Windows. Balas HANYA satu objek JSON: {\"nama\":\"huruf-angka-strip tanpa spasi\",\"bahasa\":\"powershell|python\",\"deskripsi\":\"satu kalimat\",\"kode\":\"kode lengkap\"}. \
Aturan keras: bekerja hanya pada berkas lokal di folder kerja skrip (direktori saat ini) atau argumen yang diberikan; TANPA jaringan, TANPA registri/tugas terjadwal/layanan, TANPA eval/exec/enkode base64, TANPA kata sandi/token; keluaran dicetak ke stdout; maksimal 60 baris; skrip dijalankan dengan satu argumen teks opsional (PowerShell: $args[0], Python: sys.argv[1]).";
    let mut galat = String::new();
    for percobaan in 0..2 {
        progres::set(job, if percobaan == 0 { 30 } else { 60 }, "menulis dan memeriksa skrip");
        let u = if galat.is_empty() { permintaan.to_string() } else { format!("{permintaan}\n\nSKRIP SEBELUMNYA DITOLAK: {galat}. Perbaiki dan balas JSON saja.") };
        let raw = match crate::keahlian::minta_model(http, sys, &u).await {
            Ok(t) => t,
            Err(e) => {
                progres::gagal(job, "gagal menulis");
                return format!("Belum bisa membuat skrip: {e}.");
            }
        };
        match alat::parse_json(&raw).ok_or_else(|| "keluaran model bukan JSON".to_string()).and_then(|v| validasi(&v)) {
            Ok(s) => {
                let teks = pratinjau(&s);
                if let Ok(mut d) = DRAFT.lock() {
                    *d = Some(s);
                }
                progres::selesai(job, "skrip siap diperiksa");
                return teks;
            }
            Err(e) => galat = e,
        }
    }
    progres::gagal(job, "skrip ditolak");
    format!("Skrip ditolak pemindai: {galat}. Coba jelaskan dengan kata lain.")
}

fn cari(nama: &str) -> Option<Skrip> {
    daftar().into_iter().find(|s| s.nama.eq_ignore_ascii_case(nama.trim()))
}

/// Jalankan skrip tersimpan (pemanggil sudah mendapat izin Bos lewat `alat::assess`).
pub fn eksekusi(nama: &str, argumen: &str) -> String {
    let Some(s) = cari(nama) else { return format!("Skrip '{nama}' tidak ditemukan.") };
    if hash(&s.kode) != s.sha256 {
        return "Ditolak: isi skrip berubah sejak disimpan (hash tidak cocok).".into();
    }
    if let Err(e) = pindai(&s.kode) {
        return format!("Ditolak pemindai: {e}");
    }
    let dir = data_dir().join("skrip_kerja").join(&s.nama);
    if std::fs::create_dir_all(&dir).is_err() {
        return "Gagal menyiapkan folder kerja skrip.".into();
    }
    let ext = if s.bahasa == "python" { "py" } else { "ps1" };
    let berkas = dir.join(format!("skrip.{ext}"));
    if std::fs::write(&berkas, &s.kode).is_err() {
        return "Gagal menulis skrip ke folder kerja.".into();
    }
    let mut c = if s.bahasa == "python" {
        let mut c = std::process::Command::new("python");
        c.args(["-I", "-X", "utf8"]).arg(&berkas);
        c
    } else {
        let mut c = std::process::Command::new("powershell");
        c.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"]).arg(&berkas);
        c
    };
    let argumen: String = argumen.chars().filter(|c| !c.is_control()).take(200).collect();
    if !argumen.is_empty() {
        c.arg(argumen);
    }
    c.current_dir(&dir).env_clear().stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    for k in ["PATH", "SystemRoot", "windir", "TEMP", "TMP", "COMSPEC", "PATHEXT", "USERPROFILE"] {
        if let Ok(v) = std::env::var(k) {
            c.env(k, v);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let mut anak = match c.spawn() {
        Ok(a) => a,
        Err(e) => return format!("Tidak bisa menjalankan {}: {e}", s.bahasa),
    };
    let baca = |mut h: Box<dyn Read + Send>| std::thread::spawn(move || {
        let mut b = vec![];
        let _ = h.by_ref().take(20_000).read_to_end(&mut b);
        String::from_utf8_lossy(&b).into_owned()
    });
    let (o, e) = (baca(Box::new(anak.stdout.take().unwrap())), baca(Box::new(anak.stderr.take().unwrap())));
    let mulai = Instant::now();
    let kode = loop {
        match anak.try_wait() {
            Ok(Some(st)) => break st.code().map(|c| c.to_string()).unwrap_or_else(|| "?".into()),
            Ok(None) if mulai.elapsed() > Duration::from_secs(BATAS_DETIK) => {
                let _ = anak.kill();
                let _ = anak.wait();
                break format!("dihentikan setelah {BATAS_DETIK} dtk");
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(_) => break "?".into(),
        }
    };
    let (out, err) = (o.join().unwrap_or_default(), e.join().unwrap_or_default());
    format!("▶ skrip {} selesai (kode {kode}, {} dtk)\n{}{}", s.nama, mulai.elapsed().as_secs(), crate::research::truncate(out.trim(), 1500), if err.trim().is_empty() { String::new() } else { format!("\n[stderr] {}", crate::research::truncate(err.trim(), 500)) })
}

/// Perintah alat `skrip`: buat <permintaan> | simpan | batal | daftar | tampil <nama> | hapus <nama> | jalankan <nama> [argumen]
pub async fn jalankan(http: &reqwest::Client, arg: &str) -> String {
    let arg = arg.trim();
    let (cmd, rest) = arg.split_once(' ').map(|(a, b)| (a, b.trim())).unwrap_or((arg, ""));
    match cmd {
        "buat" => buat(http, rest).await,
        "simpan" => {
            let d = DRAFT.lock().ok().and_then(|mut d| d.take());
            let Some(s) = d else { return "Tidak ada rancangan skrip yang menunggu.".into() };
            let mut v = daftar();
            v.retain(|x| x.nama != s.nama);
            let n = s.nama.clone();
            v.push(s);
            if tulis(&v) { format!("✅ Skrip '{n}' disimpan. Jalankan: 'skrip jalankan {n}' (selalu minta izinmu).") } else { "Gagal menyimpan skrip.".into() }
        }
        "batal" => {
            if let Ok(mut d) = DRAFT.lock() {
                *d = None;
            }
            "Rancangan skrip dibatalkan.".into()
        }
        "tampil" => cari(rest).map(|s| pratinjau(&s)).unwrap_or_else(|| format!("Skrip '{rest}' tidak ditemukan.")),
        "hapus" => {
            let mut v = daftar();
            let n = v.len();
            v.retain(|x| !x.nama.eq_ignore_ascii_case(rest));
            let _ = std::fs::remove_dir_all(data_dir().join("skrip_kerja").join(rest));
            if v.len() < n && tulis(&v) { format!("Skrip '{rest}' dihapus.") } else { format!("Skrip '{rest}' tidak ditemukan.") }
        }
        "jalankan" => {
            let (nama, argumen) = rest.split_once(' ').map(|(a, b)| (a, b.trim())).unwrap_or((rest, ""));
            let (nama, argumen) = (nama.to_string(), argumen.to_string());
            tokio::task::spawn_blocking(move || eksekusi(&nama, &argumen)).await.unwrap_or_else(|_| "skrip gagal dijalankan".into())
        }
        _ => {
            let v = daftar();
            if v.is_empty() {
                return "Belum ada skrip. Minta: 'buat skrip python hitung baris semua file .rs'.".into();
            }
            format!("📜 Skrip ({}):\n{}", v.len(), v.iter().map(|s| format!("• {} [{}] — {}", s.nama, s.bahasa, s.deskripsi)).collect::<Vec<_>>().join("\n"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    static BERKAS: Mutex<()> = Mutex::new(()); // dua tes menulis skrip.json yang sama

    #[test]
    fn pemindai_menolak_pola_berbahaya() {
        for k in ["Invoke-WebRequest http://x", "import requests", "iex $x", "reg add HKCU\\x", "os.system('dir')", "[Convert]::FromBase64String($a)", "schtasks /create"] {
            assert!(pindai(k).is_err(), "{k}");
        }
        assert!(pindai("Get-ChildItem | Measure-Object").unwrap().is_empty());
        assert!(pindai("Remove-Item x.txt").unwrap().contains(&"remove-item"));
    }

    #[test]
    fn validasi_dan_hash() {
        let ok = json!({"nama": "hitung-baris", "bahasa": "python", "deskripsi": "d", "kode": "print(sum(1 for _ in open('a.txt')))"});
        let s = validasi(&ok).unwrap();
        assert_eq!(s.sha256, hash(&s.kode));
        assert!(validasi(&json!({"nama": "x y", "bahasa": "python", "kode": "print(1)"})).is_err());
        assert!(validasi(&json!({"nama": "abc", "bahasa": "bash", "kode": "ls"})).is_err());
        assert!(validasi(&json!({"nama": "abc", "bahasa": "python", "kode": "import socket"})).is_err());
    }

    #[test]
    fn skrip_berubah_ditolak_dan_batas_waktu_aman() {
        let _k = BERKAS.lock().unwrap_or_else(|e| e.into_inner());
        let mut s = validasi(&json!({"nama": "uji-hash", "bahasa": "powershell", "deskripsi": "d", "kode": "Write-Output 'halo'"})).unwrap();
        s.kode.push_str("\nWrite-Output 'diubah'");
        assert!(tulis(&[s]));
        assert!(eksekusi("uji-hash", "").contains("hash tidak cocok"));
    }

    #[test]
    fn skrip_powershell_jalan_di_folder_kerja_dan_batas_env() {
        let _k = BERKAS.lock().unwrap_or_else(|e| e.into_inner());
        let s = validasi(&json!({"nama": "uji-jalan", "bahasa": "powershell", "deskripsi": "d", "kode": "Write-Output \"halo $($args[0]) $env:NOVA_UJI_RAHASIA\""})).unwrap();
        assert!(tulis(&[s]));
        std::env::set_var("NOVA_UJI_RAHASIA", "RAHASIA");
        let r = eksekusi("uji-jalan", "dunia");
        assert!(r.contains("halo dunia") && !r.contains("RAHASIA"), "{r}");
    }
}
