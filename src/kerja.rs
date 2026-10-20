//! Pet pembantu kerja: rekap kerja harian/mingguan, rutinitas mulai/akhiri kerja, istirahat, dan penjaga fokus jam kerja.
//! Semua dari data lokal yang sudah ada (nama aplikasi + jam, fokus, tugas, to-do, git); tanpa model, tanpa membaca isi layar/ketikan.
use chrono::{Datelike, Duration as Cd, Local};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

fn hm(s: &str) -> i32 {
    let (h, m) = s.split_once(':').unwrap_or((s, "0"));
    h.trim().parse::<i32>().unwrap_or(0) * 60 + m.trim().parse::<i32>().unwrap_or(0)
}

fn dalam_jam_kerja() -> bool {
    let m = crate::assistant::day_min();
    crate::assistant::schedule().work.iter().any(|(a, b)| m >= hm(a) && m < hm(b))
}

fn durasi(detik: i64) -> String {
    let m = detik / 60;
    if m >= 60 { format!("{}j {}m", m / 60, m % 60) } else { format!("{m}m") }
}

const HARI: [&str; 7] = ["Sen", "Sel", "Rab", "Kam", "Jum", "Sab", "Min"];

/// Detik pemakaian per mode kerja untuk satu hari, dan jam paling produktif (bukan Media/Santai).
fn pemakaian(hari: &str) -> (HashMap<&'static str, i64>, Option<(u32, i64)>) {
    let mut per: HashMap<&'static str, i64> = HashMap::new();
    let mut per_jam: HashMap<u32, i64> = HashMap::new();
    if let Ok(c) = crate::assistant::db() {
        if let Ok(mut st) = c.prepare("SELECT app, hour, secs FROM usage WHERE day=?1") {
            if let Ok(rows) = st.query_map([hari], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?))) {
                for (app, jam, secs) in rows.flatten() {
                    let mode = crate::konteks::classify(&app);
                    *per.entry(mode.label()).or_insert(0) += secs;
                    if !matches!(mode, crate::konteks::Mode::Media | crate::konteks::Mode::Lain) {
                        *per_jam.entry(jam as u32).or_insert(0) += secs;
                    }
                }
            }
        }
    }
    (per, per_jam.into_iter().max_by_key(|x| x.1))
}

/// Rentang aktif (jam pertama–terakhir) dari catatan menit aktif untuk hari itu.
fn rentang(hari: &str) -> Option<(String, String)> {
    let c = crate::assistant::db().ok()?;
    let awal = chrono::NaiveDate::parse_from_str(hari, "%Y-%m-%d").ok()?.and_hms_opt(0, 0, 0)?.and_local_timezone(Local).single()?.timestamp() / 60;
    let (a, b): (Option<i64>, Option<i64>) = c.query_row("SELECT MIN(min), MAX(min) FROM act WHERE min >= ?1 AND min < ?2", [awal, awal + 1440], |r| Ok((r.get(0)?, r.get(1)?))).ok()?;
    let f = |m: i64| chrono::DateTime::from_timestamp(m * 60, 0).map(|t| t.with_timezone(&Local).format("%H:%M").to_string());
    Some((f(a?)?, f(b?)?))
}

fn fokus_hari() -> (u32, u32) {
    crate::widget::fokus_hari_ini()
}

fn commit_hari_ini() -> Option<(String, Vec<String>)> {
    let dir = crate::proyek::active()?;
    let mut c = std::process::Command::new("git");
    c.args(["log", "--since=midnight", "--pretty=%s", "-n", "8"]).current_dir(&dir);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let o = c.output().ok().filter(|o| o.status.success())?;
    let l: Vec<String> = String::from_utf8_lossy(&o.stdout).lines().map(|s| s.chars().take(70).collect()).collect();
    Some((dir.file_name()?.to_string_lossy().into_owned(), l))
}

pub fn rekap_hari() -> String {
    let now = Local::now();
    let hari = now.format("%Y-%m-%d").to_string();
    let (per, puncak) = pemakaian(&hari);
    let total: i64 = per.values().sum();
    let mut o = format!("📒 Rekap kerja — {} {}\n", HARI[now.weekday().num_days_from_monday() as usize], now.format("%d/%m"));
    match rentang(&hari) {
        Some((a, b)) if total > 0 => o.push_str(&format!("⏱ Aktif {} (mulai {a}, terakhir {b})\n", durasi(total))),
        _ => o.push_str("⏱ Belum ada data aktivitas hari ini.\n"),
    }
    let (fm, fs) = fokus_hari();
    o.push_str(&format!("🎯 Fokus: {fm} menit hari ini ({fs} sesi total)\n"));
    if !per.is_empty() {
        let mut v: Vec<_> = per.into_iter().filter(|x| x.1 >= 300).collect();
        v.sort_by_key(|x| std::cmp::Reverse(x.1));
        o.push_str(&format!("🧰 Waktu per jenis: {}\n", v.iter().take(4).map(|(m, s)| format!("{} {}", m.trim_start_matches("Mode "), durasi(*s))).collect::<Vec<_>>().join(" · ")));
    }
    match crate::tugas::today() {
        Some((t, true)) => o.push_str(&format!("✅ Tugas utama: {t} (selesai)\n")),
        Some((t, false)) => o.push_str(&format!("⬜ Tugas utama: {t} (belum selesai)\n")),
        None => o.push_str("⬜ Belum ada tugas utama hari ini.\n"),
    }
    let todo = crate::widget::todo_list();
    let (selesai, terbuka): (Vec<_>, Vec<_>) = todo.iter().partition(|t| t["selesai"].as_bool() == Some(true));
    if !todo.is_empty() {
        o.push_str(&format!("☑ To-do: {} selesai, {} belum", selesai.len(), terbuka.len()));
        if !terbuka.is_empty() {
            o.push_str(&format!(" → besok mulai dari: {}", terbuka.iter().take(3).filter_map(|t| t["teks"].as_str()).collect::<Vec<_>>().join("; ")));
        }
        o.push('\n');
    }
    if let Some((proyek, c)) = commit_hari_ini() {
        if !c.is_empty() {
            o.push_str(&format!("🔧 {} commit di '{proyek}': {}\n", c.len(), c.iter().take(3).cloned().collect::<Vec<_>>().join(" | ")));
        }
    }
    if let Some(l) = crate::engineer::waktu_proyek(1).strip_prefix("⏱ Waktu per proyek (hari ini):\n") {
        o.push_str(&format!("📁 Waktu per proyek: {}\n", l.replace("\n", " ").replace("• ", "")));
    }
    if let Some((jam, _)) = puncak {
        o.push_str(&format!("💡 Jam paling produktif hari ini sekitar {jam:02}:00 (bukan hiburan).\n"));
    }
    o.push_str("Ringkasan ini dihitung lokal dari nama aplikasi & jam; tidak membaca isi layar atau ketikan.");
    o
}

pub fn rekap_minggu() -> String {
    let now = Local::now();
    let mut o = String::from("📅 Rekap 7 hari terakhir (waktu aktif non-hiburan)\n");
    let (mut total, mut hari_aktif) = (0i64, 0);
    for i in (0..7).rev() {
        let d = now - Cd::days(i);
        let (per, _) = pemakaian(&d.format("%Y-%m-%d").to_string());
        let kerja: i64 = per.iter().filter(|(m, _)| !matches!(**m, "Mode Media" | "Santai")).map(|(_, s)| *s).sum();
        total += kerja;
        if kerja > 600 {
            hari_aktif += 1;
        }
        let bar = "▇".repeat(((kerja / 1800).min(16)) as usize);
        o.push_str(&format!("{} {:>6} {bar}\n", HARI[d.weekday().num_days_from_monday() as usize], durasi(kerja)));
    }
    o.push_str(&format!("Total {} di {hari_aktif} hari aktif", durasi(total)));
    if hari_aktif > 0 {
        o.push_str(&format!(" (rata-rata {} per hari aktif)", durasi(total / hari_aktif)));
    }
    o.push_str(&format!(". Fokus Pomodoro hari ini: {} menit.", fokus_hari().0));
    o
}

// ---------- rutinitas ----------

fn mulai_fokus(menit: u64) -> String {
    crate::widget::aksi(&serde_json::json!({"aksi": "fokus_mulai", "menit": menit}))
}

pub fn mulai_kerja() -> String {
    let mut o = String::from("🌅 Mulai kerja\n");
    match crate::tugas::today() {
        Some((t, false)) => o.push_str(&format!("🎯 Tugas utama: {t}\n")),
        Some((t, true)) => o.push_str(&format!("✅ Tugas utama '{t}' sudah selesai. Tambah satu lagi? (tugas utama <teks>)\n")),
        None => o.push_str("🎯 Belum ada tugas utama. Ketik: tugas utama <yang terpenting hari ini>\n"),
    }
    let todo = crate::widget::todo_list();
    let terbuka: Vec<&str> = todo.iter().filter(|t| t["selesai"].as_bool() != Some(true)).filter_map(|t| t["teks"].as_str()).take(3).collect();
    if !terbuka.is_empty() {
        o.push_str(&format!("☑ To-do terbuka: {}\n", terbuka.join("; ")));
    }
    let agenda = crate::assistant::report("agenda");
    if !agenda.trim().is_empty() {
        o.push_str(&format!("📅 {}\n", agenda.lines().take(3).collect::<Vec<_>>().join(" · ")));
    }
    if let Some(nama) = crate::proyek::active().and_then(|d| d.file_name().map(|n| n.to_string_lossy().into_owned())) {
        let _ = crate::sistem::kode(&nama);
        o.push_str(&format!("🧰 Membuka proyek aktif '{nama}' di VS Code\n"));
    }
    o.push_str(&mulai_fokus(25));
    o
}

pub fn akhiri_kerja() -> String {
    let _ = crate::widget::aksi(&serde_json::json!({"aksi": "fokus_stop"}));
    format!("🌇 Akhiri kerja\n{}\nSelamat istirahat; besok kita lanjut dari daftar di atas.", rekap_hari())
}

pub fn istirahat() -> String {
    let _ = crate::widget::aksi(&serde_json::json!({"aksi": "fokus_stop"}));
    let r = crate::assistant::add_reminder("5 menit lagi kembali kerja");
    format!("☕ Istirahat 5 menit: berdiri, regangkan leher dan punggung, minum air, tatap objek jauh 20 detik.\n{r}")
}

// ---------- penjaga fokus ----------

fn off_file() -> std::path::PathBuf {
    crate::config::data_dir().join("penjaga_fokus_off")
}

pub fn set_penjaga(on: bool) -> String {
    if on {
        let _ = std::fs::remove_file(off_file());
        "Penjaga fokus: AKTIF. Bila Bos menonton video >20 menit di jam kerja (jadwal.json), kuingatkan lembut (maks 2x/hari). Di luar jam kerja aku diam.".into()
    } else {
        let _ = std::fs::write(off_file(), "");
        "Penjaga fokus: MATI.".into()
    }
}

struct Pantau {
    menit_video: u32,
    hari: i64,
    kali: u32,
    terakhir: Option<Instant>,
}

static PANTAU: Mutex<Pantau> = Mutex::new(Pantau { menit_video: 0, hari: 0, kali: 0, terakhir: None });

/// Aturan murni: ingatkan bila video terlalu lama di jam kerja, di bawah kuota, dengan jarak cukup.
pub fn perlu_ingatkan(menit_video: u32, jam_kerja: bool, kali_hari_ini: u32, menit_sejak_terakhir: Option<u64>) -> bool {
    jam_kerja && menit_video >= 20 && kali_hari_ini < 2 && menit_sejak_terakhir.is_none_or(|m| m >= 45)
}

/// Dipanggil tiap menit dari pekerjaan latar.
pub fn penjaga(tx: &crate::engine::Tx) {
    if off_file().exists() {
        return;
    }
    let kerja = dalam_jam_kerja();
    let video = crate::ambien::media() == "video";
    let mut p = PANTAU.lock().unwrap_or_else(|e| e.into_inner());
    let hari = Local::now().ordinal() as i64;
    if p.hari != hari {
        (p.hari, p.kali, p.menit_video) = (hari, 0, 0);
    }
    p.menit_video = if kerja && video { p.menit_video + 1 } else { p.menit_video.saturating_sub(2) };
    if perlu_ingatkan(p.menit_video, kerja, p.kali, p.terakhir.map(|t| t.elapsed().as_secs() / 60)) && !crate::bandit::quiet() {
        p.kali += 1;
        p.terakhir = Some(Instant::now());
        p.menit_video = 0;
        drop(p);
        let tugas = crate::tugas::today().filter(|t| !t.1).map(|t| format!(" Tugas utama: {}.", t.0)).unwrap_or_default();
        crate::notify::toast("NOVA", "Sudah cukup lama nonton video di jam kerja. Kembali fokus?");
        tx.send(crate::engine::Event::Ask { question: format!("Sudah ±20 menit nonton video di jam kerja.{tugas} Mau mulai fokus 25 menit?"), options: vec!["mulai fokus 25 menit".into(), "tunda fokus 10 menit".into()] });
    }
}

/// Perintah alat `kerja`: mulai | akhir | rekap | minggu | istirahat | fokus | tunda | penjaga on|off
pub fn jalankan(arg: &str) -> String {
    let a = arg.trim().to_lowercase();
    match a.as_str() {
        "mulai" => mulai_kerja(),
        "akhir" => akhiri_kerja(),
        "minggu" => rekap_minggu(),
        "proyek" => crate::engineer::waktu_proyek(1),
        "proyek minggu" => crate::engineer::waktu_proyek(7),
        "ekspor" => crate::engineer::ekspor_laporan(),
        "istirahat" => istirahat(),
        "fokus" => mulai_fokus(25),
        "tunda" => {
            let _ = crate::assistant::add_reminder("10 menit lagi kembali fokus");
            "Oke, kuingatkan 10 menit lagi.".into()
        }
        "senyap on" => {
            crate::gerbang::set_senyap(true);
            "🤫 Mode senyap AKTIF: peliharaan disembunyikan dan notifikasi biasa ditahan; hanya pesan kritis (izin, baterai, peringatan sistem) yang tampil. NOVA tetap berjalan di tray. Ketik 'mode senyap off' atau pakai menu tray untuk menampilkannya lagi.".into()
        }
        "senyap off" => {
            crate::gerbang::set_senyap(false);
            "👋 Mode senyap MATI: peliharaan dan notifikasi kembali normal.".into()
        }
        "penjaga on" => set_penjaga(true),
        "penjaga off" => set_penjaga(false),
        _ => rekap_hari(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aturan_penjaga_fokus() {
        assert!(perlu_ingatkan(20, true, 0, None));
        assert!(!perlu_ingatkan(19, true, 0, None)); // belum cukup lama
        assert!(!perlu_ingatkan(30, false, 0, None)); // di luar jam kerja: diam
        assert!(!perlu_ingatkan(30, true, 2, None)); // kuota harian habis
        assert!(!perlu_ingatkan(30, true, 1, Some(10))); // terlalu dekat dengan ingatan sebelumnya
        assert!(perlu_ingatkan(30, true, 1, Some(60)));
    }

    #[test]
    fn format_durasi() {
        assert_eq!(durasi(3600 * 2 + 600), "2j 10m");
        assert_eq!(durasi(900), "15m");
    }
}
