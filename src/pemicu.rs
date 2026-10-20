//! Pemicu non-waktu: "kalau X terjadi, lakukan Y". Kejadian: baterai rendah, aplikasi dibuka, flash disk dicolok, berkas baru di folder,
//! Wi-Fi berganti. Langkahnya alat NOVA yang sudah ada (divalidasi seperti tugas terjadwal); langkah berisiko masuk antrean izin Bos.
//! Hanya memantau di dalam proses NOVA (tanpa hook sistem, tanpa merekam isi layar/ketikan).
use crate::alat;
use crate::config::data_dir;
use crate::engine::Tx;
use crate::keahlian::Langkah;
use crate::progres;
use chrono::{Datelike, Local};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Mutex;

const MAKS: usize = 15;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Kejadian {
    /// baterai_rendah | aplikasi_dibuka | flashdisk | berkas_baru | jaringan_berubah
    pub tipe: String,
    #[serde(default)]
    pub param: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Syarat {
    #[serde(default)]
    pub jam_mulai: String,
    #[serde(default)]
    pub jam_selesai: String,
    #[serde(default)]
    pub hari: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Pemicu {
    pub id: u32,
    pub nama: String,
    pub ringkas: String,
    pub kejadian: Kejadian,
    #[serde(default)]
    pub syarat: Syarat,
    pub cooldown_menit: u32,
    pub aktif: bool,
    pub langkah: Vec<Langkah>,
    pub dibuat: i64,
}

static DRAFT: Mutex<Option<Pemicu>> = Mutex::new(None);

fn file() -> std::path::PathBuf {
    data_dir().join("pemicu.json")
}

pub fn muat() -> Vec<Pemicu> {
    std::fs::read_to_string(file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn tulis(v: &[Pemicu]) -> bool {
    std::fs::write(file(), serde_json::to_string_pretty(v).unwrap_or_default()).is_ok()
}

pub fn ada_draft() -> bool {
    DRAFT.lock().map(|d| d.is_some()).unwrap_or(false)
}

// ---------- templat ----------

pub struct Templat {
    pub nama: &'static str,
    pub ringkas: &'static str,
    pub tipe: &'static str,
    pub param: &'static str,
    pub cooldown: u32,
    pub langkah: &'static [(&'static str, &'static str)],
}

pub const TEMPLAT: &[Templat] = &[
    Templat { nama: "Baterai rendah", ringkas: "Ingatkan colok charger saat baterai ≤ 20%", tipe: "baterai_rendah", param: "20", cooldown: 30, langkah: &[("pesan", "Baterai tinggal sedikit: colok charger dan simpan pekerjaan.")] },
    Templat { nama: "Flash disk dicolok", ringkas: "Beri tahu saat ada flash disk baru", tipe: "flashdisk", param: "", cooldown: 1, langkah: &[("pesan", "Flash disk {peristiwa} terdeteksi.")] },
    Templat { nama: "Unduhan baru", ringkas: "Beri tahu berkas baru di folder Unduhan", tipe: "berkas_baru", param: "%DOWNLOADS%", cooldown: 2, langkah: &[("pesan", "Berkas baru di Unduhan: {peristiwa}")] },
    Templat { nama: "VS Code dibuka", ringkas: "Sapa saat mulai koding", tipe: "aplikasi_dibuka", param: "code", cooldown: 120, langkah: &[("pesan", "Mulai sesi koding. Semangat, Bos! Ketik 'mulai kerja' bila ingin kucatat waktunya.")] },
    Templat { nama: "Wi-Fi berganti", ringkas: "Beri tahu saat pindah jaringan Wi-Fi", tipe: "jaringan_berubah", param: "", cooldown: 5, langkah: &[("pesan", "Sekarang tersambung ke Wi-Fi: {peristiwa}")] },
];

fn dari_templat(i: usize) -> Option<Pemicu> {
    let t = TEMPLAT.get(i)?;
    let param = if t.param == "%DOWNLOADS%" { std::env::var("USERPROFILE").map(|p| format!("{p}\\Downloads")).unwrap_or_default() } else { t.param.to_string() };
    Some(Pemicu {
        id: muat().iter().map(|p| p.id).max().unwrap_or(0) + 1,
        nama: t.nama.into(),
        ringkas: t.ringkas.into(),
        kejadian: Kejadian { tipe: t.tipe.into(), param },
        syarat: Syarat::default(),
        cooldown_menit: t.cooldown,
        aktif: true,
        langkah: t.langkah.iter().map(|(a, g)| Langkah { alat: a.to_string(), arg: g.to_string(), jika: None }).collect(),
        dibuat: Local::now().timestamp(),
    })
}

pub fn buat_dari_templat(i: usize) -> (bool, String) {
    let Some(p) = dari_templat(i) else { return (false, "Templat tidak ditemukan.".into()) };
    set_draft(p);
    let r = simpan();
    (r.starts_with('✅'), r)
}

fn set_draft(p: Pemicu) {
    if let Ok(mut d) = DRAFT.lock() {
        *d = Some(p);
    }
}

// ---------- validasi & tampilan ----------

fn jam_ok(s: &str) -> bool {
    s.is_empty() || chrono::NaiveTime::parse_from_str(s.trim(), "%H:%M").is_ok()
}

fn validasi(v: &Value, id: u32) -> Result<Pemicu, String> {
    let nama = v["nama"].as_str().unwrap_or("").trim().to_string();
    if !(3..=40).contains(&nama.chars().count()) || !nama.chars().all(|c| c.is_alphanumeric() || " -_".contains(c)) {
        return Err("nama pemicu tidak valid (3-40 huruf/angka)".into());
    }
    let tipe = v["kejadian"]["tipe"].as_str().unwrap_or("").to_lowercase();
    let param = v["kejadian"]["param"].as_str().map(|s| s.trim().to_string()).unwrap_or_else(|| v["kejadian"]["param"].as_u64().map(|n| n.to_string()).unwrap_or_default());
    match tipe.as_str() {
        "baterai_rendah" => {
            let n: u32 = if param.is_empty() { 20 } else { param.parse().map_err(|_| "ambang baterai harus angka 5-50")? };
            if !(5..=50).contains(&n) {
                return Err("ambang baterai harus 5-50".into());
            }
        }
        "aplikasi_dibuka" if param.chars().count() < 2 || param.chars().count() > 40 => return Err("nama aplikasi 2-40 huruf".into()),
        "berkas_baru" if !std::path::Path::new(&param).is_dir() => return Err(format!("folder '{param}' tidak ada")),
        "aplikasi_dibuka" | "berkas_baru" | "flashdisk" | "jaringan_berubah" => {}
        _ => return Err("kejadian harus baterai_rendah, aplikasi_dibuka, flashdisk, berkas_baru, atau jaringan_berubah".into()),
    }
    let s = &v["syarat"];
    let syarat = Syarat {
        jam_mulai: s["jam_mulai"].as_str().unwrap_or("").trim().into(),
        jam_selesai: s["jam_selesai"].as_str().unwrap_or("").trim().into(),
        hari: s["hari"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(|h| h.to_lowercase())).collect()).unwrap_or_default(),
    };
    if !jam_ok(&syarat.jam_mulai) || !jam_ok(&syarat.jam_selesai) || syarat.hari.iter().any(|h| hari_idx(h).is_none()) {
        return Err("syarat jam harus HH:MM dan hari senin..minggu".into());
    }
    let cooldown = v["cooldown_menit"].as_u64().unwrap_or(10).clamp(1, 1440) as u32;
    let langkah = crate::penjadwal::validasi_langkah(&v["langkah"], 4)?;
    Ok(Pemicu { id, nama, ringkas: v["ringkas"].as_str().unwrap_or("").chars().take(160).collect(), kejadian: Kejadian { tipe, param }, syarat, cooldown_menit: cooldown, aktif: true, langkah, dibuat: Local::now().timestamp() })
}

fn hari_idx(h: &str) -> Option<u32> {
    ["senin", "selasa", "rabu", "kamis", "jumat", "sabtu", "minggu"].iter().position(|x| *x == h.trim()).map(|i| i as u32)
}

fn teks_kejadian(k: &Kejadian) -> String {
    match k.tipe.as_str() {
        "baterai_rendah" => format!("baterai ≤ {}% dan tidak diisi", if k.param.is_empty() { "20" } else { &k.param }),
        "aplikasi_dibuka" => format!("aplikasi '{}' dibuka", k.param),
        "flashdisk" => "flash disk dicolok".into(),
        "berkas_baru" => format!("berkas baru di {}", k.param),
        _ => "Wi-Fi berganti".into(),
    }
}

fn teks_syarat(s: &Syarat) -> String {
    let mut p = vec![];
    if !s.jam_mulai.is_empty() && !s.jam_selesai.is_empty() {
        p.push(format!("jam {}–{}", s.jam_mulai, s.jam_selesai));
    }
    if !s.hari.is_empty() {
        p.push(s.hari.join(", "));
    }
    if p.is_empty() { String::new() } else { format!(", hanya {}", p.join(" · ")) }
}

fn pratinjau(p: &Pemicu) -> String {
    let langkah = p
        .langkah
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let r = match alat::assess(&alat::Call { tool: l.alat.clone(), arg: l.arg.clone() }) {
                alat::Risk::Safe => "jalan otomatis",
                alat::Risk::Confirm(_) => "butuh izin Bos setiap kali",
                alat::Risk::Block(_) => "DIBLOKIR",
            };
            format!("  {}. {} {}{} [{r}]", i + 1, l.alat, l.arg, l.jika.as_ref().map(|j| format!(" (hanya jika {j})")).unwrap_or_default())
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("🔔 Rancangan pemicu: {}\n{}\nBila: {}{} (jeda {} menit)\nLangkah:\n{langkah}\nPemantauan hanya di dalam NOVA; langkah berisiko menunggu izin Bos. Buat atau batalkan di bawah.", p.nama, p.ringkas, teks_kejadian(&p.kejadian), teks_syarat(&p.syarat), p.cooldown_menit)
}

// ---------- pembuatan lewat model ----------

pub async fn buat(http: &reqwest::Client, permintaan: &str) -> String {
    let permintaan = permintaan.trim();
    if permintaan.chars().count() < 8 {
        return "Jelaskan pemicunya, mis. 'kalau baterai di bawah 15% ingatkan aku colok charger'.".into();
    }
    if muat().len() >= MAKS {
        return format!("Sudah ada {MAKS} pemicu (batas). Hapus yang tak dipakai dulu.");
    }
    let job = progres::mulai(&format!("Pemicu: {}", crate::research::truncate(permintaan, 40)), "memahami permintaan");
    let sys = format!(
        "Kamu perancang pemicu untuk asisten NOVA. Ubah permintaan Bos menjadi satu objek JSON (tanpa teks lain): \
{{\"nama\":\"nama pendek\",\"ringkas\":\"satu kalimat\",\"kejadian\":{{\"tipe\":\"baterai_rendah|aplikasi_dibuka|flashdisk|berkas_baru|jaringan_berubah\",\"param\":\"ambang % | nama aplikasi | path folder\"}},\
\"syarat\":{{\"jam_mulai\":\"HH:MM\",\"jam_selesai\":\"HH:MM\",\"hari\":[\"senin\"]}},\"cooldown_menit\":10,\"langkah\":[{{\"alat\":\"nama alat\",\"arg\":\"argumen\"}}]}}. \
Kolom syarat opsional (kosongkan bila tak diminta). Maksimal 4 langkah. Untuk menampilkan teks pakai alat 'pesan'. Di argumen boleh memakai {{peristiwa}} \
(diganti: drive flash disk, nama berkas baru, nama aplikasi, atau nama Wi-Fi). Hanya pakai alat berikut (nama persis):\n{}\n{}\nJangan membuat kode. Jangan memakai alat: {}.",
        crate::registry::bantuan(),
        crate::kondisi::PETUNJUK_MODEL,
        crate::penjadwal::DILARANG.join(", ")
    );
    progres::set(job, 25, "merancang pemicu dengan model");
    let id = muat().iter().map(|p| p.id).max().unwrap_or(0) + 1;
    let mut galat = String::new();
    for percobaan in 0..2 {
        let u = if galat.is_empty() { permintaan.to_string() } else { format!("{permintaan}\n\nRANCANGAN SEBELUMNYA DITOLAK: {galat}. Perbaiki dan balas JSON saja.") };
        let raw = match crate::keahlian::minta_model(http, &sys, &u).await {
            Ok(t) => t,
            Err(e) => {
                progres::gagal(job, "gagal merancang");
                return format!("Belum bisa membuat pemicu: {e}.");
            }
        };
        progres::set(job, if percobaan == 0 { 65 } else { 80 }, "memeriksa pemicu dan keamanan");
        match alat::parse_json(&raw).ok_or_else(|| "keluaran model bukan JSON".to_string()).and_then(|v| validasi(&v, id)) {
            Ok(p) => {
                let teks = pratinjau(&p);
                set_draft(p);
                progres::selesai(job, "rancangan siap");
                return teks;
            }
            Err(e) => galat = e,
        }
    }
    progres::gagal(job, "rancangan ditolak");
    format!("Rancangan pemicu ditolak: {galat}. Coba jelaskan dengan kata lain.")
}

pub fn simpan() -> String {
    let d = DRAFT.lock().ok().and_then(|mut d| d.take());
    let Some(p) = d else { return "Tidak ada rancangan pemicu yang menunggu.".into() };
    let mut v = muat();
    let (nama, id) = (p.nama.clone(), p.id.max(v.iter().map(|x| x.id).max().unwrap_or(0) + 1));
    v.push(Pemicu { id, ..p });
    if !tulis(&v) {
        return "Gagal menyimpan pemicu.".into();
    }
    format!("✅ Pemicu '{nama}' aktif (nomor {id}). Lihat: 'daftar pemicu'.")
}

pub fn hapus(id: u32) -> String {
    let mut v = muat();
    let n = v.len();
    v.retain(|p| p.id != id);
    if v.len() == n {
        return format!("Pemicu {id} tidak ditemukan.");
    }
    tulis(&v);
    format!("Pemicu {id} dihapus.")
}

pub fn ubah_aktif(id: u32, aktif: bool) -> (bool, String) {
    let mut v = muat();
    let Some(p) = v.iter_mut().find(|p| p.id == id) else { return (false, format!("Pemicu {id} tidak ditemukan.")) };
    p.aktif = aktif;
    let n = p.nama.clone();
    tulis(&v);
    (true, format!("Pemicu '{n}' {}.", if aktif { "diaktifkan" } else { "dinonaktifkan" }))
}

pub fn daftar_json() -> Vec<Value> {
    muat()
        .iter()
        .map(|p| {
            json!({"id": p.id, "nama": p.nama, "ringkas": p.ringkas, "kejadian": teks_kejadian(&p.kejadian), "syarat": teks_syarat(&p.syarat), "cooldown": p.cooldown_menit, "aktif": p.aktif,
                   "langkah": p.langkah.iter().map(|l| format!("{} {}{}", l.alat, l.arg, l.jika.as_ref().map(|j| format!(" [jika {j}]")).unwrap_or_default())).collect::<Vec<_>>(), "terakhir": crate::otomasi::riwayat(Some(&format!("p{}", p.id)), 1).into_iter().next()})
        })
        .collect()
}

/// Perintah alat `pemicu`: buat <permintaan> | simpan | batal | daftar | hapus <id> | aktif <id> | nonaktif <id> | templat [n] | riwayat
pub async fn jalankan(http: &reqwest::Client, arg: &str) -> String {
    let arg = arg.trim();
    let (cmd, rest) = arg.split_once(' ').map(|(a, b)| (a, b.trim())).unwrap_or((arg, ""));
    let id: u32 = rest.parse().unwrap_or(0);
    match cmd {
        "buat" => buat(http, rest).await,
        "simpan" => simpan(),
        "batal" => {
            if let Ok(mut d) = DRAFT.lock() {
                *d = None;
            }
            "Rancangan pemicu dibatalkan.".into()
        }
        "hapus" => hapus(id),
        "aktif" | "nonaktif" => ubah_aktif(id, cmd == "aktif").1,
        "riwayat" => crate::otomasi::riwayat_teks(None),
        "templat" => match rest.parse::<usize>().ok().and_then(|n| dari_templat(n.saturating_sub(1))) {
            Some(p) => {
                let t = pratinjau(&p);
                set_draft(p);
                t
            }
            None => format!("🧩 Templat pemicu (ketik: pemicu templat <nomor>):\n{}", TEMPLAT.iter().enumerate().map(|(i, t)| format!("{}. {} — {}", i + 1, t.nama, t.ringkas)).collect::<Vec<_>>().join("\n")),
        },
        _ => {
            let v = muat();
            if v.is_empty() {
                return "Belum ada pemicu. Minta: 'kalau baterai di bawah 15% ingatkan aku' atau 'pemicu templat'.".into();
            }
            format!("🔔 Pemicu ({}):\n{}", v.len(), v.iter().map(|p| format!("{}. {} — bila {}{} [{}]", p.id, p.nama, teks_kejadian(&p.kejadian), teks_syarat(&p.syarat), if p.aktif { "aktif" } else { "mati" })).collect::<Vec<_>>().join("\n"))
        }
    }
}

// ---------- pemantau ----------

/// Syarat jam/hari terpenuhi pada waktu `now`? Rentang yang melewati tengah malam (22:00–06:00) didukung.
pub fn syarat_ok(s: &Syarat, now: chrono::DateTime<Local>) -> bool {
    if !s.hari.is_empty() && !s.hari.iter().any(|h| hari_idx(h) == Some(now.weekday().num_days_from_monday())) {
        return false;
    }
    let (Ok(a), Ok(b)) = (chrono::NaiveTime::parse_from_str(&s.jam_mulai, "%H:%M"), chrono::NaiveTime::parse_from_str(&s.jam_selesai, "%H:%M")) else { return true };
    let t = now.time();
    if a <= b { t >= a && t <= b } else { t >= a || t <= b }
}

#[derive(Default)]
struct Keadaan {
    baterai_di_atas: HashMap<u32, bool>,
    app_ada: HashMap<u32, bool>,
    drive: Option<u32>,
    berkas: HashMap<u32, std::collections::HashSet<String>>,
    ssid: Option<String>,
    terakhir: HashMap<u32, i64>,
}

#[cfg(windows)]
fn flash_disk() -> u32 {
    use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};
    let mask = unsafe { GetLogicalDrives() };
    let mut out = 0u32;
    for i in 0..26 {
        if mask & (1 << i) != 0 {
            let root: Vec<u16> = format!("{}:\\", (b'A' + i as u8) as char).encode_utf16().chain(std::iter::once(0)).collect();
            if unsafe { GetDriveTypeW(root.as_ptr()) } == 2 {
                out |= 1 << i;
            }
        }
    }
    out
}

#[cfg(not(windows))]
fn flash_disk() -> u32 {
    0
}

pub(crate) fn ssid() -> Option<String> {
    let t = crate::sistem::wifi();
    t.lines().find_map(|l| l.strip_prefix("Tersambung: ")).map(|r| r.split(" · ").next().unwrap_or("").trim().to_string())
}

fn daftar_berkas(dir: &str) -> std::collections::HashSet<String> {
    std::fs::read_dir(dir).map(|d| d.flatten().take(2000).map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default()
}

/// Bersihkan teks kejadian sebelum disisipkan ke argumen langkah (satu baris, pendek).
fn bersih(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).take(100).collect()
}

/// Satu putaran pemantauan: kembalikan (pemicu, peristiwa) yang terpenuhi sekarang.
fn periksa(ks: &mut Keadaan, semua: &[Pemicu], putaran: u64) -> Vec<(Pemicu, String)> {
    let now = Local::now();
    let mut hasil = vec![];
    let drive_kini = semua.iter().any(|p| p.aktif && p.kejadian.tipe == "flashdisk").then(flash_disk);
    let ssid_kini = (putaran % 4 == 0 && semua.iter().any(|p| p.aktif && p.kejadian.tipe == "jaringan_berubah")).then(ssid);
    let app = crate::konteks::current_app().to_lowercase();
    for p in semua.iter().filter(|p| p.aktif) {
        let peristiwa: Option<String> = match p.kejadian.tipe.as_str() {
            "baterai_rendah" => {
                let ambang: f32 = p.kejadian.param.parse().unwrap_or(20.0);
                match crate::baterai::read() {
                    Some(b) => {
                        let atas = b.ac || b.pct > ambang;
                        let sebelum = ks.baterai_di_atas.insert(p.id, atas).unwrap_or(true);
                        (sebelum && !atas).then(|| format!("{:.0}%", b.pct))
                    }
                    None => None,
                }
            }
            "aplikasi_dibuka" => {
                let ada = !app.is_empty() && app.contains(&p.kejadian.param.to_lowercase());
                let sebelum = ks.app_ada.insert(p.id, ada).unwrap_or(ada);
                (ada && !sebelum).then(|| p.kejadian.param.clone())
            }
            "flashdisk" => drive_kini.and_then(|kini| {
                let lama = ks.drive.unwrap_or(kini);
                let baru = kini & !lama;
                (baru != 0).then(|| format!("{}:", (b'A' + baru.trailing_zeros() as u8) as char))
            }),
            "berkas_baru" => {
                let kini = daftar_berkas(&p.kejadian.param);
                let lama = ks.berkas.insert(p.id, kini.clone());
                lama.and_then(|l| kini.difference(&l).next().cloned())
            }
            "jaringan_berubah" => ssid_kini.clone().flatten().and_then(|s| {
                let lama = ks.ssid.replace(s.clone());
                (lama.is_some() && lama.as_deref() != Some(&s)).then_some(s)
            }),
            _ => None,
        };
        if let Some(e) = peristiwa {
            let sela = now.timestamp() - ks.terakhir.get(&p.id).copied().unwrap_or(0);
            if sela >= p.cooldown_menit as i64 * 60 && syarat_ok(&p.syarat, now) {
                ks.terakhir.insert(p.id, now.timestamp());
                hasil.push((p.clone(), bersih(&e)));
            }
        }
    }
    if let Some(d) = drive_kini {
        ks.drive = Some(d);
    }
    hasil
}

/// Mulai pemantau di thread sendiri (ringan: ±15 detik sekali, hanya bila ada pemicu aktif).
pub fn mulai(tx: Tx) {
    std::thread::spawn(move || {
        let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() else { return };
        let http = crate::research::client(30);
        let mut ks = Keadaan::default();
        let mut putaran = 0u64;
        std::thread::sleep(std::time::Duration::from_secs(20));
        loop {
            let semua = muat();
            if semua.iter().any(|p| p.aktif) && !crate::gerbang::senyap() {
                for (p, e) in periksa(&mut ks, &semua, putaran) {
                    let langkah: Vec<Langkah> = p.langkah.iter().map(|l| Langkah { alat: l.alat.clone(), arg: l.arg.replace("{peristiwa}", &e), jika: l.jika.clone() }).collect();
                    let (ringkas, _) = rt.block_on(crate::penjadwal::run_langkah(&http, &p.nama, &format!("p{}", p.id), &langkah));
                    tx.notice(format!("🔔 {}: {}", p.nama, crate::research::truncate(&ringkas, 400)));
                }
            }
            putaran += 1;
            std::thread::sleep(std::time::Duration::from_secs(15));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn syarat_jam_dan_hari() {
        let s = Syarat { jam_mulai: "22:00".into(), jam_selesai: "06:00".into(), hari: vec![] };
        let jam = |h, m| Local.with_ymd_and_hms(2026, 10, 20, h, m, 0).unwrap(); // Selasa
        assert!(syarat_ok(&s, jam(23, 0)) && syarat_ok(&s, jam(5, 0)) && !syarat_ok(&s, jam(12, 0)));
        let h = Syarat { hari: vec!["selasa".into()], ..Default::default() };
        assert!(syarat_ok(&h, jam(9, 0)));
        let r = Syarat { hari: vec!["rabu".into()], ..Default::default() };
        assert!(!syarat_ok(&r, jam(9, 0)));
    }

    #[test]
    fn validasi_pemicu() {
        let ok = json!({"nama": "Baterai", "kejadian": {"tipe": "baterai_rendah", "param": "15"}, "langkah": [{"alat": "pesan", "arg": "colok {peristiwa}"}]});
        assert!(validasi(&ok, 1).is_ok());
        let angka = json!({"nama": "Baterai", "kejadian": {"tipe": "baterai_rendah", "param": 15}, "langkah": [{"alat": "pesan", "arg": "x"}]});
        assert!(validasi(&angka, 1).is_ok());
        let salah = json!({"nama": "Baterai", "kejadian": {"tipe": "baterai_rendah", "param": "99"}, "langkah": [{"alat": "pesan", "arg": "x"}]});
        assert!(validasi(&salah, 1).is_err());
        let rekursi = json!({"nama": "Rekursi", "kejadian": {"tipe": "flashdisk"}, "langkah": [{"alat": "pemicu", "arg": "buat x"}]});
        assert!(validasi(&rekursi, 1).is_err());
        let bahaya = json!({"nama": "Bahaya", "kejadian": {"tipe": "flashdisk"}, "langkah": [{"alat": "shell", "arg": "rm -rf /"}]});
        assert!(validasi(&bahaya, 1).is_err());
        let folder = json!({"nama": "Folder", "kejadian": {"tipe": "berkas_baru", "param": "Z:\\tidak\\ada"}, "langkah": [{"alat": "pesan", "arg": "x"}]});
        assert!(validasi(&folder, 1).is_err());
    }

    #[test]
    fn templat_valid() {
        for (i, t) in TEMPLAT.iter().enumerate() {
            let p = dari_templat(i).unwrap();
            assert!(!p.langkah.is_empty(), "{}", t.nama);
            for l in &p.langkah {
                assert!(crate::registry::ada(&l.alat) && !crate::penjadwal::DILARANG.contains(&l.alat.as_str()), "{}", t.nama);
            }
        }
    }

    #[test]
    fn peristiwa_dibersihkan() {
        assert_eq!(bersih("a\nb\u{7}c"), "abc");
    }
}
