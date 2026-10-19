//! Penjadwal NOVA di atas Windows Task Scheduler (tanpa admin, berjalan sebagai akun Bos, hak terendah).
//! 1) Tugas "NOVA\Mulai": menyalakan NOVA saat login dan menghidupkannya lagi otomatis bila proses mati karena galat.
//! 2) Tugas buatan Bos lewat bahasa sehari-hari: model (Gemini/Llama) merancang jadwal + langkah dari ALAT NOVA YANG SUDAH ADA,
//!    divalidasi, ditampilkan, dan baru dibuat setelah Bos setuju. Task Scheduler menjalankan `nova --tugas <id>`; langkah yang butuh
//!    izin TIDAK dijalankan otomatis (hanya diberitahukan), jadi tugas terjadwal tidak bisa melakukan hal berisiko diam-diam.
use crate::alat::{self, Call, Risk};
use crate::config::data_dir;
use crate::keahlian::Langkah;
use crate::progres;
use chrono::{Datelike, Duration as Cd, Local, NaiveDate, NaiveTime, Weekday};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Mutex;

const FOLDER: &str = "NOVA";
const MAKS_TUGAS: usize = 20;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Jadwal {
    /// harian | mingguan | sekali | menit | login
    pub tipe: String,
    #[serde(default)]
    pub jam: String,
    #[serde(default)]
    pub hari: Vec<String>,
    #[serde(default)]
    pub tanggal: String,
    #[serde(default)]
    pub tiap_menit: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Tugas {
    pub id: u32,
    pub nama: String,
    pub ringkas: String,
    pub jadwal: Jadwal,
    pub langkah: Vec<Langkah>,
    pub dibuat: i64,
}

static DRAFT: Mutex<Option<Tugas>> = Mutex::new(None);

fn file() -> std::path::PathBuf {
    data_dir().join("tugas_terjadwal.json")
}

pub fn muat() -> Vec<Tugas> {
    std::fs::read_to_string(file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn tulis(v: &[Tugas]) -> bool {
    std::fs::write(file(), serde_json::to_string_pretty(v).unwrap_or_default()).is_ok()
}

pub fn ada_draft() -> bool {
    DRAFT.lock().map(|d| d.is_some()).unwrap_or(false)
}

// ---------- XML Task Scheduler ----------

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn pengguna() -> String {
    let u = std::env::var("USERNAME").unwrap_or_default();
    match std::env::var("USERDOMAIN") {
        Ok(d) if !d.is_empty() => format!("{d}\\{u}"),
        _ => u,
    }
}

fn hari_xml(h: &str) -> Option<&'static str> {
    Some(match h.trim().to_lowercase().as_str() {
        "senin" | "monday" => "Monday",
        "selasa" | "tuesday" => "Tuesday",
        "rabu" | "wednesday" => "Wednesday",
        "kamis" | "thursday" => "Thursday",
        "jumat" | "jum'at" | "friday" => "Friday",
        "sabtu" | "saturday" => "Saturday",
        "minggu" | "ahad" | "sunday" => "Sunday",
        _ => return None,
    })
}

fn weekday(h: &str) -> Option<Weekday> {
    Some(match hari_xml(h)? {
        "Monday" => Weekday::Mon,
        "Tuesday" => Weekday::Tue,
        "Wednesday" => Weekday::Wed,
        "Thursday" => Weekday::Thu,
        "Friday" => Weekday::Fri,
        "Saturday" => Weekday::Sat,
        _ => Weekday::Sun,
    })
}

fn waktu(jam: &str) -> Option<NaiveTime> {
    let (h, m) = jam.trim().split_once([':', '.'])?;
    NaiveTime::from_hms_opt(h.trim().parse().ok()?, m.trim().parse().ok()?, 0)
}

/// Kejadian berikutnya dari jadwal (waktu lokal), untuk batas awal XML dan untuk ditampilkan.
pub fn berikutnya(j: &Jadwal, sekarang: chrono::DateTime<Local>) -> Option<chrono::NaiveDateTime> {
    let now = sekarang.naive_local();
    match j.tipe.as_str() {
        "harian" => {
            let t = waktu(&j.jam)?;
            let hari_ini = now.date().and_time(t);
            Some(if hari_ini > now { hari_ini } else { hari_ini + Cd::days(1) })
        }
        "mingguan" => {
            let t = waktu(&j.jam)?;
            let hari: Vec<Weekday> = j.hari.iter().filter_map(|h| weekday(h)).collect();
            (0..8).map(|d| (now.date() + Cd::days(d)).and_time(t)).find(|c| *c > now && hari.contains(&c.date().weekday()))
        }
        "sekali" => Some(NaiveDate::parse_from_str(&j.tanggal, "%Y-%m-%d").ok()?.and_time(waktu(&j.jam)?)),
        "menit" => Some(now + Cd::minutes(1)),
        _ => None,
    }
}

fn trigger_xml(j: &Jadwal) -> Result<String, String> {
    let nxt = berikutnya(j, Local::now());
    let mulai = |d: Option<chrono::NaiveDateTime>| d.map(|x| x.format("%Y-%m-%dT%H:%M:%S").to_string()).ok_or("jadwal tidak valid".to_string());
    Ok(match j.tipe.as_str() {
        "harian" => format!("<CalendarTrigger><StartBoundary>{}</StartBoundary><Enabled>true</Enabled><ScheduleByDay><DaysInterval>1</DaysInterval></ScheduleByDay></CalendarTrigger>", mulai(nxt)?),
        "mingguan" => {
            let hari: String = j.hari.iter().filter_map(|h| hari_xml(h)).map(|h| format!("<{h}/>")).collect();
            format!("<CalendarTrigger><StartBoundary>{}</StartBoundary><Enabled>true</Enabled><ScheduleByWeek><WeeksInterval>1</WeeksInterval><DaysOfWeek>{hari}</DaysOfWeek></ScheduleByWeek></CalendarTrigger>", mulai(nxt)?)
        }
        "sekali" => format!("<TimeTrigger><StartBoundary>{}</StartBoundary><Enabled>true</Enabled></TimeTrigger>", mulai(nxt)?),
        "menit" => format!("<TimeTrigger><StartBoundary>{}</StartBoundary><Enabled>true</Enabled><Repetition><Interval>PT{}M</Interval></Repetition></TimeTrigger>", mulai(nxt)?, j.tiap_menit),
        "login" => format!("<LogonTrigger><Enabled>true</Enabled><UserId>{}</UserId><Delay>PT30S</Delay></LogonTrigger>", esc(&pengguna())),
        _ => return Err("tipe jadwal tidak dikenal".into()),
    })
}

fn task_xml(trigger: &str, exe: &str, args: &str, deskripsi: &str, mulai_ulang: bool) -> String {
    let restart = if mulai_ulang { "<RestartOnFailure><Interval>PT1M</Interval><Count>5</Count></RestartOnFailure>" } else { "" };
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo><Description>{}</Description></RegistrationInfo>
  <Triggers>{trigger}</Triggers>
  <Principals><Principal id="Author"><UserId>{}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal></Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>true</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>7</Priority>
    {restart}
  </Settings>
  <Actions Context="Author"><Exec><Command>{}</Command><Arguments>{}</Arguments></Exec></Actions>
</Task>"#,
        esc(deskripsi),
        esc(&pengguna()),
        esc(exe),
        esc(args)
    )
}

fn schtasks(args: &[&str]) -> Result<String, String> {
    let mut c = std::process::Command::new("schtasks");
    c.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let o = c.output().map_err(|e| format!("schtasks tidak bisa dijalankan: {e}"))?;
    if o.status.success() {
        Ok(String::from_utf8_lossy(&o.stdout).into_owned())
    } else {
        Err(format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)).trim().chars().take(200).collect())
    }
}

fn buat_task(nama: &str, xml: &str) -> Result<(), String> {
    // schtasks mensyaratkan XML UTF-16 LE ber-BOM.
    let mut bytes: Vec<u8> = vec![0xFF, 0xFE];
    for u in xml.encode_utf16() {
        bytes.extend_from_slice(&u.to_le_bytes());
    }
    let tmp = std::env::temp_dir().join(format!("nova-task-{}.xml", std::process::id()));
    std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    let r = schtasks(&["/Create", "/TN", nama, "/XML", &tmp.to_string_lossy(), "/F"]);
    let _ = std::fs::remove_file(&tmp);
    r.map(|_| ())
}

fn hapus_task(nama: &str) -> bool {
    schtasks(&["/Delete", "/TN", nama, "/F"]).is_ok()
}

fn ada_task(nama: &str) -> bool {
    schtasks(&["/Query", "/TN", nama]).is_ok()
}

fn exe() -> String {
    std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default()
}

// ---------- NOVA menyala otomatis (tugas "NOVA\Mulai") ----------

fn nama_mulai() -> String {
    format!("{FOLDER}\\Mulai")
}

pub fn autostart_aktif() -> bool {
    ada_task(&nama_mulai())
}

/// Nyalakan/matikan NOVA otomatis saat login lewat Task Scheduler (dengan mulai ulang otomatis). Entri Run lama dibersihkan agar tak ganda.
pub fn autostart_set(on: bool) -> bool {
    const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    let hapus_run = || {
        let _ = std::process::Command::new("reg").args(["delete", RUN_KEY, "/v", "NOVA", "/f"]).output();
    };
    if on {
        let j = Jadwal { tipe: "login".into(), jam: String::new(), hari: vec![], tanggal: String::new(), tiap_menit: 0 };
        let xml = task_xml(&trigger_xml(&j).unwrap_or_default(), &exe(), "", "Menyalakan NOVA saat masuk Windows; dihidupkan lagi otomatis bila berhenti karena galat.", true);
        match buat_task(&nama_mulai(), &xml) {
            Ok(()) => {
                hapus_run();
                true
            }
            Err(_) => std::process::Command::new("reg").args(["add", RUN_KEY, "/v", "NOVA", "/t", "REG_SZ", "/d", &format!("\"{}\"", exe()), "/f"]).output().map(|o| o.status.success()).unwrap_or(false),
        }
    } else {
        hapus_run();
        let _ = hapus_task(&nama_mulai());
        true
    }
}

// ---------- validasi & pratinjau ----------

const DILARANG: &[&str] = &["keahlian", "jadwalkan", "pembaruan", "kirim", "bersih", "urungkan"];

fn validasi(v: &Value, id: u32) -> Result<Tugas, String> {
    let nama = v["nama"].as_str().unwrap_or("").trim().to_string();
    if !(3..=40).contains(&nama.chars().count()) || !nama.chars().all(|c| c.is_alphanumeric() || " -_".contains(c)) {
        return Err("nama tugas tidak valid (3-40 huruf/angka)".into());
    }
    let j = &v["jadwal"];
    let tipe = j["tipe"].as_str().unwrap_or("").to_lowercase();
    let jam = j["jam"].as_str().unwrap_or("").trim().to_string();
    let hari: Vec<String> = j["hari"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_lowercase())).collect()).unwrap_or_default();
    let tanggal = j["tanggal"].as_str().unwrap_or("").trim().to_string();
    let tiap = j["tiap_menit"].as_u64().unwrap_or(0) as u32;
    let jadwal = Jadwal { tipe: tipe.clone(), jam: jam.clone(), hari: hari.clone(), tanggal, tiap_menit: tiap };
    match tipe.as_str() {
        "harian" if waktu(&jam).is_none() => return Err("jam harus berformat HH:MM".into()),
        "mingguan" if waktu(&jam).is_none() || hari.is_empty() || hari.iter().any(|h| hari_xml(h).is_none()) => return Err("jadwal mingguan butuh jam HH:MM dan nama hari (senin..minggu)".into()),
        "sekali" if berikutnya(&jadwal, Local::now()).is_none_or(|t| t <= Local::now().naive_local()) => return Err("waktu 'sekali' harus tanggal dan jam di masa depan".into()),
        "menit" if !(15..=1440).contains(&tiap) => return Err("interval menit harus 15-1440".into()),
        "harian" | "mingguan" | "sekali" | "menit" | "login" => {}
        _ => return Err("tipe jadwal harus harian, mingguan, sekali, menit, atau login".into()),
    }
    let l = v["langkah"].as_array().ok_or("langkah kosong")?;
    if l.is_empty() || l.len() > 5 {
        return Err("butuh 1-5 langkah".into());
    }
    let mut langkah = vec![];
    for (i, x) in l.iter().enumerate() {
        let (a, arg) = (x["alat"].as_str().unwrap_or("").trim().to_lowercase(), x["arg"].as_str().unwrap_or("").to_string());
        if DILARANG.contains(&a.as_str()) || !crate::registry::ada(&a) {
            return Err(format!("langkah {} memakai alat yang tidak boleh/tidak ada: '{a}'", i + 1));
        }
        if arg.chars().count() > 300 || arg.contains(['\n', '\r']) {
            return Err(format!("argumen langkah {} tidak valid", i + 1));
        }
        if let Risk::Block(r) = alat::assess(&Call { tool: a.clone(), arg: arg.clone() }) {
            return Err(format!("langkah {} ditolak keamanan: {r}", i + 1));
        }
        langkah.push(Langkah { alat: a, arg });
    }
    Ok(Tugas { id, nama, ringkas: v["ringkas"].as_str().unwrap_or("").chars().take(160).collect(), jadwal, langkah, dibuat: Local::now().timestamp() })
}

fn teks_jadwal(j: &Jadwal) -> String {
    match j.tipe.as_str() {
        "harian" => format!("setiap hari pukul {}", j.jam),
        "mingguan" => format!("setiap {} pukul {}", j.hari.join(", "), j.jam),
        "sekali" => format!("sekali pada {} pukul {}", j.tanggal, j.jam),
        "menit" => format!("setiap {} menit", j.tiap_menit),
        _ => "setiap kali Bos masuk Windows".into(),
    }
}

fn pratinjau(t: &Tugas) -> String {
    let langkah = t
        .langkah
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let r = match alat::assess(&Call { tool: l.alat.clone(), arg: l.arg.clone() }) {
                Risk::Safe => "jalan otomatis",
                Risk::Confirm(_) => "butuh izin: hanya diberitahukan, tidak dijalankan otomatis",
                Risk::Block(_) => "DIBLOKIR",
            };
            format!("  {}. {} {} [{r}]", i + 1, l.alat, l.arg)
        })
        .collect::<Vec<_>>()
        .join("\n");
    let nxt = berikutnya(&t.jadwal, Local::now()).map(|d| d.format("%d/%m %H:%M").to_string()).unwrap_or_else(|| "saat login berikutnya".into());
    format!("⏰ Rancangan tugas terjadwal: {}\n{}\nJadwal: {} (berikutnya {nxt})\nLangkah:\n{langkah}\nDijalankan Windows Task Scheduler sebagai akun Bos (hak terendah); langkah berisiko tidak dijalankan sendiri. Buat atau batalkan di bawah.", t.nama, t.ringkas, teks_jadwal(&t.jadwal))
}

// ---------- pembuatan lewat model ----------

pub async fn buat(http: &reqwest::Client, permintaan: &str) -> String {
    let permintaan = permintaan.trim();
    if permintaan.chars().count() < 8 {
        return "Jelaskan jadwalnya, mis. 'jadwalkan setiap hari kerja jam 08:00 tampilkan cuaca Jakarta dan agenda'.".into();
    }
    if muat().len() >= MAKS_TUGAS {
        return format!("Sudah ada {MAKS_TUGAS} tugas terjadwal (batas). Hapus yang tak dipakai dulu.");
    }
    let job = progres::mulai(&format!("Jadwal: {}", crate::research::truncate(permintaan, 40)), "memahami permintaan");
    let sekarang = Local::now();
    let sys = format!(
        "Kamu perancang tugas terjadwal untuk asisten NOVA. Ubah permintaan Bos menjadi satu objek JSON (tanpa teks lain): \
{{\"nama\":\"nama pendek\",\"ringkas\":\"satu kalimat\",\"jadwal\":{{\"tipe\":\"harian|mingguan|sekali|menit|login\",\"jam\":\"HH:MM\",\"hari\":[\"senin\"],\"tanggal\":\"YYYY-MM-DD\",\"tiap_menit\":30}},\"langkah\":[{{\"alat\":\"nama alat\",\"arg\":\"argumen\"}}]}}. \
Isi hanya kolom jadwal yang relevan untuk tipenya. Maksimal 5 langkah. Untuk sekadar mengingatkan/menampilkan teks pakai alat 'pesan' dengan arg berupa teks pengingat. \
Sekarang: {} ({}). Hanya pakai alat berikut (nama persis):\n{}\n\
Jangan membuat kode. Jangan memakai alat: {}.",
        sekarang.format("%Y-%m-%d %H:%M"),
        ["Senin", "Selasa", "Rabu", "Kamis", "Jumat", "Sabtu", "Minggu"][sekarang.weekday().num_days_from_monday() as usize],
        crate::registry::bantuan(),
        DILARANG.join(", ")
    );
    progres::set(job, 25, "merancang jadwal dengan model");
    let id = muat().iter().map(|t| t.id).max().unwrap_or(0) + 1;
    let mut galat = String::new();
    for percobaan in 0..2 {
        let u = if galat.is_empty() { permintaan.to_string() } else { format!("{permintaan}\n\nRANCANGAN SEBELUMNYA DITOLAK: {galat}. Perbaiki dan balas JSON saja.") };
        let raw = match crate::keahlian::minta_model(http, &sys, &u).await {
            Ok(t) => t,
            Err(e) => {
                progres::gagal(job, "gagal merancang");
                return format!("Belum bisa membuat jadwal: {e}.");
            }
        };
        progres::set(job, if percobaan == 0 { 65 } else { 80 }, "memeriksa jadwal dan keamanan");
        match alat::parse_json(&raw).ok_or_else(|| "keluaran model bukan JSON".to_string()).and_then(|v| validasi(&v, id)) {
            Ok(t) => {
                progres::set(job, 92, "menunggu persetujuan Bos");
                let teks = pratinjau(&t);
                if let Ok(mut d) = DRAFT.lock() {
                    *d = Some(t);
                }
                progres::selesai(job, "rancangan siap");
                return teks;
            }
            Err(e) => galat = e,
        }
    }
    progres::gagal(job, "rancangan ditolak");
    format!("Rancangan jadwal ditolak: {galat}. Coba jelaskan dengan kata lain.")
}

fn nama_tugas(id: u32) -> String {
    format!("{FOLDER}\\Tugas-{id}")
}

fn simpan() -> String {
    let d = DRAFT.lock().ok().and_then(|mut d| d.take());
    let Some(t) = d else { return "Tidak ada rancangan jadwal yang menunggu.".into() };
    let xml = match trigger_xml(&t.jadwal) {
        Ok(tr) => task_xml(&tr, &exe(), &format!("--tugas {}", t.id), &format!("NOVA: {}", t.nama), false),
        Err(e) => return format!("Gagal menyusun jadwal: {e}"),
    };
    if let Err(e) = buat_task(&nama_tugas(t.id), &xml) {
        return format!("Windows Task Scheduler menolak membuat tugas: {e}");
    }
    let mut v = muat();
    let (nama, kapan) = (t.nama.clone(), teks_jadwal(&t.jadwal));
    v.push(t);
    if !tulis(&v) {
        return "Tugas dibuat di Windows tetapi gagal menyimpan daftar NOVA.".into();
    }
    format!("✅ Tugas '{nama}' dijadwalkan {kapan}. Lihat: 'daftar tugas terjadwal'. Coba sekarang: 'jalankan tugas terjadwal {}'.", v.last().map(|t| t.id).unwrap_or(0))
}

fn cari(v: &[Tugas], rest: &str) -> Option<Tugas> {
    let r = rest.trim();
    r.parse::<u32>().ok().and_then(|n| v.iter().find(|t| t.id == n)).or_else(|| v.iter().find(|t| t.nama.eq_ignore_ascii_case(r))).cloned()
}

/// Perintah alat `jadwalkan`: buat <permintaan> | simpan | batal | daftar | hapus <id|nama> | jalankan <id|nama> | latar on|off
pub async fn jalankan(http: &reqwest::Client, arg: &str) -> String {
    let arg = arg.trim();
    let (cmd, rest) = arg.split_once(' ').map(|(a, b)| (a, b.trim())).unwrap_or((arg, ""));
    match cmd {
        "buat" => buat(http, rest).await,
        "simpan" => simpan(),
        "batal" => {
            if let Ok(mut d) = DRAFT.lock() {
                *d = None;
            }
            "Rancangan jadwal dibatalkan.".into()
        }
        "hapus" => {
            let mut v = muat();
            let Some(t) = cari(&v, rest) else { return format!("Tugas '{rest}' tidak ditemukan.") };
            let _ = hapus_task(&nama_tugas(t.id));
            v.retain(|x| x.id != t.id);
            tulis(&v);
            format!("Tugas '{}' dihapus dari jadwal.", t.nama)
        }
        "jalankan" => {
            let v = muat();
            let Some(t) = cari(&v, rest) else { return format!("Tugas '{rest}' tidak ditemukan.") };
            run_tugas(http, &t, false).await
        }
        "latar" => {
            let on = rest != "off";
            if autostart_set(on) {
                if on { "NOVA akan menyala otomatis saat login lewat Task Scheduler, dan dihidupkan lagi bila berhenti karena galat.".into() } else { "Nyala otomatis NOVA dimatikan.".into() }
            } else {
                "Gagal mengubah tugas nyala otomatis.".into()
            }
        }
        _ => {
            let v = muat();
            let latar = if autostart_aktif() { "aktif (Task Scheduler, mulai ulang otomatis)" } else { "belum aktif" };
            if v.is_empty() {
                return format!("Belum ada tugas terjadwal. Nyala otomatis NOVA: {latar}.\nMinta: 'jadwalkan setiap hari jam 08:00 tampilkan cuaca Jakarta'.");
            }
            format!("⏰ Tugas terjadwal ({}); nyala otomatis NOVA: {latar}\n{}", v.len(), v.iter().map(|t| format!("{}. {} — {} [{} langkah]", t.id, t.nama, teks_jadwal(&t.jadwal), t.langkah.len())).collect::<Vec<_>>().join("\n"))
        }
    }
}

// ---------- eksekusi (dipanggil Task Scheduler lewat `nova --tugas <id>`) ----------

async fn run_tugas(http: &reqwest::Client, t: &Tugas, otomatis: bool) -> String {
    let (mut hasil, mut dilewati) = (vec![], vec![]);
    for l in &t.langkah {
        let c = Call { tool: l.alat.clone(), arg: l.arg.clone() };
        match alat::assess(&c) {
            Risk::Safe => hasil.push(format!("{}: {}", l.alat, crate::research::truncate(&Box::pin(alat::run(&c, http)).await, 500))),
            Risk::Confirm(r) => dilewati.push(format!("{} ({r})", l.alat)),
            Risk::Block(r) => dilewati.push(format!("{} (diblokir: {r})", l.alat)),
        }
    }
    let mut ringkas = hasil.join("\n");
    if !dilewati.is_empty() {
        ringkas.push_str(&format!("\nDilewati karena butuh izin Bos: {}", dilewati.join("; ")));
    }
    if otomatis {
        crate::notify::kirim(&format!("NOVA - {}", t.nama), &crate::research::truncate(&ringkas, 240));
        let baris = serde_json::json!({"id": t.id, "nama": t.nama, "waktu": Local::now().to_rfc3339(), "hasil": ringkas});
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(data_dir().join("tugas_hasil.jsonl")) {
            use std::io::Write;
            let _ = writeln!(f, "{baris}");
        }
    }
    format!("▶ {}\n{ringkas}", t.nama)
}

/// Titik masuk `nova --tugas <id>`: jalankan satu tugas terjadwal tanpa jendela lalu keluar.
pub fn tugas_cli(id: u32) -> i32 {
    // Jejak diagnosis (Task Scheduler tidak menampilkan keluaran): berkas nova-tugas.log di folder TEMP
    let log = |m: String| {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(std::env::temp_dir().join("nova-tugas.log")) {
            let _ = writeln!(f, "{} id={id} {m}", Local::now().format("%F %T"));
        }
    };
    log(format!("mulai; data={} berkas={:?}", data_dir().display(), std::fs::read_to_string(file()).map(|t| t.len()).map_err(|e| e.to_string())));
    let Some(t) = muat().into_iter().find(|t| t.id == id) else {
        log("tugas tidak ditemukan".into());
        return 2;
    };
    let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() else { return 3 };
    let http = crate::research::client(30);
    rt.block_on(run_tugas(&http, &t, true));
    0
}

/// Hasil tugas otomatis yang belum diteruskan ke obrolan (dipanggil berkala saat NOVA berjalan).
pub fn hasil_baru() -> Vec<String> {
    let p = data_dir().join("tugas_hasil.jsonl");
    let off = data_dir().join("tugas_hasil.offset");
    let Ok(isi) = std::fs::read_to_string(&p) else { return vec![] };
    let mulai: usize = std::fs::read_to_string(&off).ok().and_then(|t| t.trim().parse().ok()).unwrap_or(0);
    let mulai = if mulai > isi.len() { 0 } else { mulai };
    let baru: Vec<String> = isi[mulai..].lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).map(|v| format!("⏰ {}: {}", v["nama"].as_str().unwrap_or("tugas"), crate::research::truncate(v["hasil"].as_str().unwrap_or(""), 600))).collect();
    let _ = std::fs::write(off, isi.len().to_string());
    baru
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn jam(tipe: &str, j: &str) -> Jadwal {
        Jadwal { tipe: tipe.into(), jam: j.into(), hari: vec!["senin".into(), "jumat".into()], tanggal: String::new(), tiap_menit: 30 }
    }

    #[test]
    fn xml_tugas_valid_dan_berisi_pemicu() {
        let x = task_xml(&trigger_xml(&jam("harian", "08:30")).unwrap(), "C:\\Program Files\\NOVA\\nova.exe", "--tugas 3", "uji & <tes>", false);
        assert!(x.contains("<ScheduleByDay>") && x.contains("T08:30:00"));
        assert!(x.contains("--tugas 3") && x.contains("&amp;") && !x.contains("<tes>"));
        assert!(x.contains("LeastPrivilege")); // tidak pernah hak admin
        let w = trigger_xml(&jam("mingguan", "09:00")).unwrap();
        assert!(w.contains("<Monday/>") && w.contains("<Friday/>"));
        let m = task_xml(&trigger_xml(&jam("login", "")).unwrap(), "x.exe", "", "d", true);
        assert!(m.contains("LogonTrigger") && m.contains("RestartOnFailure"));
    }

    #[test]
    fn jadwal_berikutnya_masuk_akal() {
        let sekarang = Local::now();
        let n = berikutnya(&jam("harian", "08:30"), sekarang).unwrap();
        assert!(n > sekarang.naive_local() && n < sekarang.naive_local() + Cd::days(1) + Cd::minutes(1));
        let w = berikutnya(&jam("mingguan", "09:00"), sekarang).unwrap();
        assert!(matches!(w.date().weekday(), Weekday::Mon | Weekday::Fri));
    }

    #[test]
    fn validasi_menolak_tugas_berbahaya_atau_ngawur() {
        let ok = json!({"nama": "Cuaca pagi", "ringkas": "r", "jadwal": {"tipe": "harian", "jam": "08:00"}, "langkah": [{"alat": "cuaca", "arg": "Jakarta"}]});
        assert!(validasi(&ok, 1).is_ok());
        let kirim = json!({"nama": "Kirim", "jadwal": {"tipe": "harian", "jam": "08:00"}, "langkah": [{"alat": "kirim", "arg": "x"}]});
        assert!(validasi(&kirim, 1).is_err());
        let shell = json!({"nama": "Bahaya", "jadwal": {"tipe": "harian", "jam": "08:00"}, "langkah": [{"alat": "shell", "arg": "rm -rf /"}]});
        assert!(validasi(&shell, 1).unwrap_err().contains("ditolak"));
        let jam_salah = json!({"nama": "Salah", "jadwal": {"tipe": "harian", "jam": "25:99"}, "langkah": [{"alat": "cuaca", "arg": "x"}]});
        assert!(validasi(&jam_salah, 1).is_err());
        let terlalu_sering = json!({"nama": "Sering", "jadwal": {"tipe": "menit", "tiap_menit": 1}, "langkah": [{"alat": "cuaca", "arg": "x"}]});
        assert!(validasi(&terlalu_sering, 1).is_err());
        let lalu = json!({"nama": "Lalu", "jadwal": {"tipe": "sekali", "jam": "08:00", "tanggal": "2020-01-01"}, "langkah": [{"alat": "cuaca", "arg": "x"}]});
        assert!(validasi(&lalu, 1).is_err());
    }
}
