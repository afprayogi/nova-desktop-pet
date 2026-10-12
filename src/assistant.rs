//! Asisten pribadi: briefing saat menyala, jadwal, pelatih tidur, pengingat, dan pembelajar kebiasaan.
//! Privasi: hanya NAMA aplikasi aktif + jam + status aktif/diam, disimpan lokal di assistant.db. Tidak ada
//! ketikan, layar, judul jendela, atau isi dokumen. Hapus dengan menghapus file assistant.db.
use crate::config;
use crate::engine::{Event, Tx};
use chrono::{Duration as Cd, Local, TimeZone, Timelike};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use sysinfo::System;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Schedule {
    pub wake: String,
    pub sleep_hours: f32,
    pub work: Vec<(String, String)>,
    pub explore: (String, String),
    /// Batas akhir explore yang disarankan agar tidur cukup.
    pub explore_cap: String,
    pub break_every_min: u32,
    /// Jam tes BEC harian (HH:MM).
    pub bec_time: String,
    /// Jumlah soal per bagian tes BEC (Listening/Reading/Grammar).
    pub bec_questions: usize,
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            wake: "07:00".into(),
            sleep_hours: 8.0,
            work: vec![("08:00".into(), "11:30".into()), ("13:00".into(), "16:30".into())],
            explore: ("19:00".into(), "24:00".into()),
            explore_cap: "22:30".into(),
            break_every_min: 50,
            bec_time: "22:00".into(),
            bec_questions: 30,
        }
    }
}

pub fn schedule() -> Schedule {
    let p = config::data_dir().join("jadwal.json");
    if let Some(s) = std::fs::read_to_string(&p).ok().and_then(|t| serde_json::from_str(&t).ok()) {
        return s;
    }
    let d = Schedule::default();
    let _ = std::fs::write(&p, serde_json::to_string_pretty(&d).unwrap_or_default());
    d
}

fn hm(s: &str) -> i32 {
    let (h, m) = s.split_once(':').unwrap_or((s, "0"));
    h.trim().parse::<i32>().unwrap_or(0) * 60 + m.trim().parse::<i32>().unwrap_or(0)
}

fn fmt(m: i32) -> String {
    let m = m.rem_euclid(1440);
    format!("{:02}:{:02}", m / 60, m % 60)
}

/// (menit tidur, menit bangun) dari jadwal; dibaca sekali.
pub fn sleep_window() -> (i32, i32) {
    static S: std::sync::OnceLock<Schedule> = std::sync::OnceLock::new();
    let s = S.get_or_init(schedule);
    (bed(s), hm(&s.wake))
}

fn bed(s: &Schedule) -> i32 {
    (hm(&s.wake) - (s.sleep_hours * 60.0) as i32).rem_euclid(1440)
}

/// Peristiwa tetap harian: (menit, kunci, teks, hanya_jika_aktif)
fn fixed(s: &Schedule) -> Vec<(i32, String, String, bool)> {
    let mut v = vec![];
    for (i, (a, b)) in s.work.iter().enumerate() {
        v.push((hm(a) - 5, format!("ws{i}"), format!("Jam kerja mulai {a} (5 menit lagi). Tentukan 1 tugas utama."), false));
        let end = if i + 1 < s.work.len() {
            format!("Jam {b}: istirahat. Makan, minum, jauhi layar sampai {}.", s.work[i + 1].0)
        } else {
            "Jam kerja selesai. Simpan pekerjaan dan catat yang belum rampung.".into()
        };
        v.push((hm(b), format!("we{i}"), end, false));
    }
    v.push((hm(&s.explore.0), "ex".into(), "Waktu explore dimulai. Pilih satu topik agar tidak melebar.".into(), false));
    v.push((hm(&s.explore_cap) - 15, "excap".into(), format!("Tutup eksplorasi 15 menit lagi ({}). Catat hasilnya.", s.explore_cap), false));
    let b = bed(s);
    v.push((b - 30, "wind".into(), "Wind-down: redupkan layar, simpan pekerjaan, siap-siap tidur.".into(), false));
    v.push((b, "bed".into(), format!("Waktunya tidur (target {}j, bangun {}). Besok kerja {}.", s.sleep_hours, s.wake, s.work[0].0), false));
    for k in 1..=3 {
        v.push((b + 30 * k, format!("late{k}"), format!("Sudah lewat jam tidur {} menit. Tidur Anda terpotong; matikan laptop sekarang.", 30 * k), true));
    }
    v
}

pub(crate) fn db() -> rusqlite::Result<Connection> {
    let c = Connection::open(config::data_dir().join("assistant.db"))?;
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS usage(day TEXT, hour INT, app TEXT, secs INT, PRIMARY KEY(day,hour,app));
         CREATE TABLE IF NOT EXISTS act(min INTEGER PRIMARY KEY);
         CREATE TABLE IF NOT EXISTS rem(id INTEGER PRIMARY KEY, due INT, text TEXT, done INT DEFAULT 0);
         CREATE TABLE IF NOT EXISTS nudge(ts INT, text TEXT);
         CREATE TABLE IF NOT EXISTS fired(key TEXT PRIMARY KEY);
         CREATE TABLE IF NOT EXISTS trans(a TEXT, b TEXT, n INT, PRIMARY KEY(a,b));",
    )?;
    Ok(c)
}

fn now_ts() -> i64 {
    Local::now().timestamp()
}

pub fn day_min() -> i32 {
    let n = Local::now();
    (n.hour() * 60 + n.minute()) as i32
}

// ---------- Sampel aktivitas ----------

#[cfg(windows)]
fn foreground(sys: &mut System) -> Option<(String, u64)> {
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
    unsafe {
        let mut li = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
        if GetLastInputInfo(&mut li) == 0 {
            return None;
        }
        let idle = (GetTickCount().wrapping_sub(li.dwTime) / 1000) as u64;
        let h = GetForegroundWindow();
        if h.is_null() {
            return Some(("(tidak ada)".into(), idle));
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(h, &mut pid);
        let p = sysinfo::Pid::from_u32(pid);
        sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[p]), true);
        let name = sys.process(p).map(|p| p.name().to_string_lossy().to_lowercase().trim_end_matches(".exe").to_string())?;
        Some((name, idle))
    }
}

#[cfg(not(windows))]
fn foreground(_sys: &mut System) -> Option<(String, u64)> {
    None
}

fn sample(c: &Connection, sys: &mut System) {
    let Some((app, idle)) = foreground(sys) else { return };
    if app.starts_with("nova") {
        crate::konteks::IDLE_SECS.store(idle, std::sync::atomic::Ordering::Relaxed);
        return;
    }
    {
        // Rantai Markov: catat perpindahan aplikasi.
        static LAST_APP: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
        if let Ok(mut l) = LAST_APP.lock() {
            if *l != app && idle <= 120 {
                crate::ml::record_transition(&l, &app);
                *l = app.clone();
            }
        }
    }
    crate::konteks::set_app(&app, idle);
    if idle > 120 {
        return;
    }
    let n = Local::now();
    let _ = c.execute(
        "INSERT INTO usage VALUES(?1,?2,?3,30) ON CONFLICT(day,hour,app) DO UPDATE SET secs=secs+30",
        (n.format("%Y-%m-%d").to_string(), n.hour() as i64, app),
    );
    let _ = c.execute("INSERT OR IGNORE INTO act VALUES(?1)", [n.timestamp() / 60]);
}

// ---------- Analisis ----------

fn active_recently(c: &Connection, mins: i64) -> bool {
    let now = now_ts() / 60;
    c.query_row("SELECT COUNT(*) FROM act WHERE min >= ?1", [now - mins], |r| r.get::<_, i64>(0)).unwrap_or(0) > 0
}

/// Menit aktif beruntun terakhir (celah <=3 menit dianggap sambungan).
fn streak(c: &Connection) -> (u32, i64) {
    let now = now_ts() / 60;
    let Ok(mut st) = c.prepare("SELECT min FROM act WHERE min >= ?1 ORDER BY min DESC") else { return (0, 0) };
    let mins: Vec<i64> = st.query_map([now - 600], |r| r.get(0)).map(|r| r.flatten().collect()).unwrap_or_default();
    if mins.is_empty() || now - mins[0] > 3 {
        return (0, 0);
    }
    let mut start = mins[0];
    for w in mins.windows(2) {
        if w[0] - w[1] > 3 {
            break;
        }
        start = w[1];
    }
    ((mins[0] - start) as u32, start)
}

/// Celah tidak-aktif terpanjang tiap hari bangun (perkiraan tidur/laptop mati), 7 hari.
fn nights(c: &Connection) -> Vec<(String, f32, String)> {
    let since = now_ts() / 60 - 8 * 1440;
    let Ok(mut st) = c.prepare("SELECT min FROM act WHERE min >= ?1 ORDER BY min") else { return vec![] };
    let mins: Vec<i64> = st.query_map([since], |r| r.get(0)).map(|r| r.flatten().collect()).unwrap_or_default();
    let mut best: std::collections::BTreeMap<String, (i64, i64)> = Default::default();
    for w in mins.windows(2) {
        let gap = w[1] - w[0];
        if gap >= 180 {
            let end = Local.timestamp_opt(w[1] * 60, 0).single();
            let Some(end) = end else { continue };
            let k = end.format("%Y-%m-%d").to_string();
            if best.get(&k).map(|b| gap > b.0 - 0).unwrap_or(true) {
                best.insert(k, (gap, w[0]));
            }
        }
    }
    best.into_iter()
        .rev()
        .take(7)
        .map(|(d, (gap, from))| {
            let f = Local.timestamp_opt(from * 60, 0).single().map(|t| t.format("%H:%M").to_string()).unwrap_or_default();
            (d, gap as f32 / 60.0, f)
        })
        .collect()
}

fn last_night(c: &Connection) -> String {
    let today = Local::now().format("%Y-%m-%d").to_string();
    match nights(c).into_iter().find(|(d, _, _)| *d == today) {
        Some((_, h, from)) => format!("Tidak aktif ~{h:.1} jam semalam (sejak {from}){}", if h < 7.0 { " — kurang dari 7 jam, usahakan tidur lebih awal malam ini." } else { "." }),
        None => "Belum ada data tidur semalam (asisten belum berjalan semalam).".into(),
    }
}

fn habit_lines(c: &Connection) -> Vec<String> {
    let today = Local::now().format("%Y-%m-%d").to_string();
    let mut out = vec![];
    let top = |sql: &str, p: &dyn rusqlite::ToSql| -> Vec<String> {
        c.prepare(sql)
            .and_then(|mut st| st.query_map([p], |r| Ok(format!("{} {}m", r.get::<_, String>(0)?, r.get::<_, i64>(1)? / 60))).map(|r| r.flatten().collect()))
            .unwrap_or_default()
    };
    let t = top("SELECT app, SUM(secs) s FROM usage WHERE day=?1 GROUP BY app ORDER BY s DESC LIMIT 4", &today);
    if !t.is_empty() {
        out.push(format!("Hari ini: {}", t.join(", ")));
    }
    let h = Local::now().hour() as i64;
    let days: i64 = c.query_row("SELECT COUNT(DISTINCT day) FROM usage WHERE hour=?1 AND day<>?2", (h, &today), |r| r.get(0)).unwrap_or(0);
    if days >= 2 {
        let apps: Vec<String> = c
            .prepare("SELECT app FROM usage WHERE hour=?1 AND day<>?2 GROUP BY app ORDER BY SUM(secs) DESC LIMIT 3")
            .and_then(|mut st| st.query_map((h, &today), |r| r.get::<_, String>(0)).map(|r| r.flatten().collect()))
            .unwrap_or_default();
        out.push(format!("Biasanya jam {h:02}: {} (dari {days} hari)", apps.join(", ")));
    }
    out
}

/// Fakta profil terpelajar untuk konteks dinamis (nama aplikasi & jam saja).
pub fn profile_facts() -> Vec<String> {
    let Ok(c) = db() else { return vec![] };
    let mut v = vec![];
    let apps: Vec<String> = c
        .prepare("SELECT app, SUM(secs) FROM usage WHERE day >= date('now','-7 day') GROUP BY app ORDER BY 2 DESC LIMIT 3")
        .and_then(|mut st| st.query_map([], |r| Ok(format!("{} {:.1}j", r.get::<_, String>(0)?, r.get::<_, i64>(1)? as f32 / 3600.0))).map(|r| r.flatten().collect()))
        .unwrap_or_default();
    if !apps.is_empty() {
        v.push(format!("Aplikasi utama 7 hari: {}", apps.join(", ")));
    }
    if let Ok(h) = c.query_row("SELECT hour FROM usage GROUP BY hour ORDER BY SUM(secs) DESC LIMIT 1", [], |r| r.get::<_, i64>(0)) {
        v.push(format!("Paling aktif sekitar jam {h:02}:00"));
    }
    let n = nights(&c);
    if !n.is_empty() {
        v.push(format!("Rata-rata tidur ~{:.1} jam (target {})", n.iter().map(|x| x.1).sum::<f32>() / n.len() as f32, schedule().sleep_hours));
    }
    let (_, _) = (0, 0);
    if let Some((next, p)) = crate::ml::predict_next(&crate::konteks::current_app()) {
        v.push(format!("Setelah {}, biasanya Bos membuka {} ({:.0}%)", crate::konteks::current_app(), next, p * 100.0));
    }
    v.push("Jadwal: kerja 08:00-11:30 & 13:00-16:30, explore malam".into());
    v
}

/// Apakah pengguna aktif dalam N menit terakhir (untuk obrolan antar agen yang tidak mengganggu).
pub fn streak_min() -> u32 {
    db().map(|c| streak(&c).0).unwrap_or(0)
}

// ---------- Pengingat ----------

pub fn has_time(text: &str) -> bool {
    parse_due(text).is_some()
}

pub fn insert_reminder(due: i64, text: &str) {
    if let Ok(c) = db() {
        let _ = c.execute("INSERT INTO rem(due,text) VALUES(?1,?2)", (due, text.trim()));
    }
}

fn parse_due(text: &str) -> Option<chrono::DateTime<Local>> {
    let l = text.to_lowercase();
    let toks: Vec<&str> = l.split_whitespace().collect();
    let now = Local::now();
    let mut due = None;
    for (i, t) in toks.iter().enumerate() {
        if *t == "menit" && i > 0 {
            if let Ok(n) = toks[i - 1].parse::<i64>() {
                due = Some(now + Cd::minutes(n));
            }
        }
        if due.is_none() {
            let tt = t.replace('.', ":");
            let hmv = tt.split_once(':').and_then(|(a, b)| Some((a.parse::<u32>().ok()?, b.parse::<u32>().ok()?))).or_else(|| {
                (i > 0 && toks[i - 1] == "jam").then(|| t.parse::<u32>().ok().map(|h| (h, 0))).flatten()
            });
            if let Some((h, m)) = hmv.filter(|(h, m)| *h < 24 && *m < 60) {
                let mut d = now.date_naive().and_hms_opt(h, m, 0).and_then(|n| Local.from_local_datetime(&n).single());
                if l.contains("besok") {
                    d = d.map(|x| x + Cd::days(1));
                }
                if let Some(x) = d {
                    due = Some(if x <= now { x + Cd::days(1) } else { x });
                }
            }
        }
    }
    due
}

pub fn add_reminder(text: &str) -> String {
    let Some(due) = parse_due(text) else { return "Waktu tidak terbaca. Contoh: 'ingatkan jam 15:30 rapat' atau 'ingatkan 20 menit lagi minum'.".into() };
    let Ok(c) = db() else { return "gagal membuka database asisten".into() };
    let _ = c.execute("INSERT INTO rem(due,text) VALUES(?1,?2)", (due.timestamp(), text.trim()));
    format!("Pengingat disimpan untuk {}: {}", due.format("%d/%m %H:%M"), text.trim())
}

// ---------- Laporan untuk alat agen ----------

pub fn report(kind: &str) -> String {
    let Ok(c) = db() else { return "database asisten tidak tersedia".into() };
    match kind {
        "agenda" => state(&c).agenda.join("\n"),
        "habit" => {
            let (s, _) = streak(&c);
            let app = crate::konteks::current_app();
            let pred = crate::ml::predict_next(&app).map(|(n, p)| format!("\nPrediksi: setelah {app}, biasanya {n} ({:.0}%)", p * 100.0)).unwrap_or_default();
            format!("Aktif beruntun: {s} menit\n{}{pred}", habit_lines(&c).join("\n"))
        }
        _ => {
            let n = nights(&c);
            if n.is_empty() {
                return "Belum cukup data. Biarkan asisten berjalan beberapa malam.".into();
            }
            let avg = n.iter().map(|x| x.1).sum::<f32>() / n.len() as f32;
            let sc = schedule();
            let mut s = n.iter().map(|(d, h, f)| format!("{d}: ~{h:.1} jam (sejak {f})")).collect::<Vec<_>>().join("\n");
            s.push_str(&format!("\nRata-rata {avg:.1} jam; target {} jam. Tidur ideal jam {} untuk bangun {}.", sc.sleep_hours, fmt(bed(&sc)), sc.wake));
            if hm(&sc.explore.1) > bed(&sc) {
                s.push_str(&format!("\nSaran: explore {}–{} membuat tidur < {} jam. Percepat selesai ke {}.", sc.explore.0, sc.explore.1, sc.sleep_hours, sc.explore_cap));
            }
            s
        }
    }
}

// ---------- Status untuk dasbor ----------

#[derive(Clone, Serialize, Default)]
pub struct AssistState {
    pub agenda: Vec<String>,
    pub sleep: String,
    pub nudges: Vec<String>,
    pub habit: Vec<String>,
    pub streak_min: u32,
}

fn state(c: &Connection) -> AssistState {
    let s = schedule();
    let m = day_min();
    let mut ev: Vec<(i32, String)> = fixed(&s).into_iter().filter(|e| !e.3 && e.0 >= m && e.0 < 1440).map(|e| (e.0, e.2)).collect();
    let today_end = Local::now().date_naive().and_hms_opt(23, 59, 59).and_then(|n| Local.from_local_datetime(&n).single()).map(|d| d.timestamp()).unwrap_or(0);
    if let Ok(mut st) = c.prepare("SELECT due, text FROM rem WHERE done=0 AND due <= ?1 ORDER BY due") {
        if let Ok(rows) = st.query_map([today_end], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))) {
            for (due, t) in rows.flatten() {
                if let Some(d) = Local.timestamp_opt(due, 0).single() {
                    ev.push(((d.hour() * 60 + d.minute()) as i32, format!("🔔 {t}")));
                }
            }
        }
    }
    let today = Local::now().date_naive();
    for e in crate::konteks::events().into_iter().filter(|e| e.start.date_naive() == today) {
        let mm = (e.start.hour() * 60 + e.start.minute()) as i32;
        if mm >= m {
            ev.push((mm, format!("📅 {}", e.title)));
        }
    }
    ev.sort_by_key(|e| e.0);
    let nudges = c
        .prepare("SELECT text FROM nudge ORDER BY ts DESC LIMIT 4")
        .and_then(|mut st| st.query_map([], |r| r.get::<_, String>(0)).map(|r| r.flatten().collect()))
        .unwrap_or_default();
    AssistState {
        agenda: ev.into_iter().take(6).map(|(m, t)| format!("{} {}", fmt(m), t.chars().take(70).collect::<String>())).collect(),
        sleep: last_night(c),
        nudges,
        habit: habit_lines(c),
        streak_min: streak(c).0,
    }
}

// ---------- Tindakan ----------

fn nudge(tx: &Tx, c: &Connection, text: &str) {
    let _ = c.execute("INSERT INTO nudge VALUES(?1,?2)", (now_ts(), text));
    crate::notify::toast("NOVA", text);
    crate::adegan::on_nudge(tx, text);
}

/// Nudge lunak yang dikendalikan bandit: diam / notifikasi halus / adegan penuh. Yang kritis memakai `nudge` biasa.
fn soft_nudge(tx: &Tx, c: &Connection, kind: &str, text: &str) {
    if crate::bandit::quiet() {
        return;
    }
    let ctx = crate::bandit::ctx(kind);
    let a = crate::bandit::choose(&ctx, &["skip", "halus", "scene"]);
    crate::bandit::expect(&ctx, &a);
    match a.as_str() {
        "skip" => {}
        "halus" => {
            let _ = c.execute("INSERT INTO nudge VALUES(?1,?2)", (now_ts(), text));
            crate::notify::toast("NOVA", text);
        }
        _ => nudge(tx, c, text),
    }
    tx.send(Event::FeedbackAsk(a != "skip"));
}

/// True sekali per kunci (de-dupe lintas modul).
pub fn once_key(key: &str) -> bool {
    db().map(|c| once(&c, key)).unwrap_or(false)
}

fn once(c: &Connection, key: &str) -> bool {
    c.execute("INSERT OR IGNORE INTO fired VALUES(?1)", [key]).map(|n| n == 1).unwrap_or(false)
}

fn briefing(tx: &Tx, c: &Connection, booted: bool) {
    let h = Local::now().hour();
    let sapa = match h { 4..=10 => "pagi", 11..=14 => "siang", 15..=17 => "sore", _ => "malam" };
    let st = state(c);
    let health = crate::monitor::summary().split(" | ").take(3).collect::<Vec<_>>().join(" | ");
    let mut t = format!("Selamat {sapa}, Bos.{}\n{health}\n{}", if booted { " Laptop baru menyala." } else { " Asisten aktif." }, st.sleep);
    if !st.agenda.is_empty() {
        t.push_str(&format!("\nAgenda: {}", st.agenda.iter().take(3).cloned().collect::<Vec<_>>().join(" · ")));
    }
    if let Some(h) = st.habit.last() {
        t.push_str(&format!("\n{h}"));
    }
    crate::notify::toast("NOVA - Briefing", &t);
    tx.notice(format!("📋 {t}"));
}

fn tick(tx: &Tx, c: &Connection, s: &Schedule) {
    for (key, text) in crate::sholat::due() {
        if once(c, &key) {
            nudge(tx, c, &text);
        }
    }
    let day = Local::now().format("%Y-%m-%d").to_string();
    let m = day_min();
    let active = active_recently(c, 3);
    for (t, key, text, only_active) in fixed(s) {
        let t = t.rem_euclid(1440);
        if m >= t && m < t + 3 && (!only_active || active) && once(c, &format!("{day}{key}")) {
            match key.as_str() {
                "ws0" if crate::tugas::today().is_none() => crate::tugas::ask_main(tx),
                "we0" | "we1" if matches!(crate::tugas::today(), Some((_, false))) => crate::tugas::check_done(tx),
                _ => nudge(tx, c, &text),
            }
        }
    }
    // Tes BEC harian (opt-in lewat /bec on): pengingat 10 menit sebelum, lalu layar terkunci di peliharaan.
    if crate::bec::enabled() && !crate::bec::done_today_pub() {
        let tm = crate::bec::test_minute();
        if m >= tm - 10 && m < tm - 7 && once(c, &format!("{day}becpre")) {
            nudge(tx, c, &format!("Tes bahasa Inggris (BEC) harian jam {:02}:{:02}. Kamu harus menyelesaikannya sebelum bisa memakai laptop lagi.", tm / 60, tm % 60));
            tx.send(Event::Ask { question: "Mulai tes BEC sekarang?".into(), options: vec!["Mulai sekarang".into(), "Nanti sesuai jadwal".into()] });
        }
        if m >= tm && m < tm + 3 && once(c, &format!("{day}becnow")) {
            crate::notify::toast("NOVA - tes BEC", "Waktunya tes bahasa Inggris harian. Layar akan terkunci sampai selesai.");
        }
    }
    // Minum air tiap jam saat Bos aktif (di antara 1 jam setelah bangun dan 30 menit sebelum tidur).
    if active && m % 60 < 3 && m >= hm(&s.wake) + 60 && m < bed(s) - 30 && once(c, &format!("{day}water{}", m / 60)) {
        soft_nudge(tx, c, "air", "Sudah satu jam. Minum segelas air putih ya, Bos. 💧");
    }
    // Acara kalender (.ics) yang mulai dalam 15 menit.
    let nowl = Local::now();
    for ev in crate::konteks::events() {
        let d = ev.start.signed_duration_since(nowl).num_minutes();
        if (0..=15).contains(&d) && once(c, &format!("{day}ics{}{}", ev.start.timestamp(), ev.title)) {
            nudge(tx, c, &format!("📅 Acara '{}' mulai {} ({} menit lagi).", ev.title, ev.start.format("%H:%M"), d));
        }
    }
    // Istirahat berkala saat aktif beruntun terlalu lama.
    let (len, start) = streak(c);
    if s.break_every_min > 0 && len >= s.break_every_min {
        let block = len / s.break_every_min;
        if once(c, &format!("brk{start}-{block}")) {
            soft_nudge(tx, c, "istirahat", &format!("Sudah {len} menit non-stop. Istirahat 5 menit: berdiri, regangkan leher/punggung, minum air, tatap objek jauh 20 detik. ☕"));
        }
    }
    // Pengingat jatuh tempo.
    if let Ok(mut st) = c.prepare("SELECT id, text FROM rem WHERE done=0 AND due <= ?1") {
        let due: Vec<(i64, String)> = st.query_map([now_ts()], |r| Ok((r.get(0)?, r.get(1)?))).map(|r| r.flatten().collect()).unwrap_or_default();
        for (id, text) in due {
            let _ = c.execute("UPDATE rem SET done=1 WHERE id=?1", [id]);
            nudge(tx, c, &format!("Pengingat: {text}"));
        }
    }
}

pub async fn run(tx: Tx) {
    let Ok(c) = db() else { return };
    crate::musik::jaga_loop();
    let _ = c.execute("DELETE FROM usage WHERE day < date('now','-90 day')", []);
    let _ = c.execute("DELETE FROM act WHERE min < ?1", [now_ts() / 60 - 30 * 1440]);
    let _ = c.execute("DELETE FROM fired WHERE key NOT LIKE ?1", [format!("{}%", Local::now().format("%Y-%m-%d"))]);
    let s = schedule();
    let mut sys = System::new();
    tokio::time::sleep(Duration::from_secs(8)).await;
    let booted = System::uptime() < 900;
    if let Some(m) = crate::buddy::add_xp("sesi") {
        tx.notice(m);
    }
    crate::makan::start(tx.clone());
    crate::widget::start(tx.clone());
    crate::hewan::start_guard(tx.clone());
    crate::harga::start(tx.clone());
    crate::analitik::start(tx.clone());
    std::thread::spawn(crate::lokal::ensure_server);
    std::thread::spawn(|| {
        crate::cadangan::run();
        crate::belajar::retrain();
    });
    briefing(&tx, &c, booted);
    crate::adegan::greet(&tx, booted);
    let http = crate::research::client(15);
    let mut n = 0u64;
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(40));
        crate::indeks::ensure_fresh();
    });
    loop {
        sample(&c, &mut sys);
        if let Some((agent, text)) = crate::konteks::mode_hint() {
            crate::adegan::play_soft(&tx, vec![(agent, text)], 1, 30);
        }
        if n % 2 == 0 {
            tick(&tx, &c, &s);
            tx.send(Event::Assist(Box::new(state(&c))));
        }
        // Obrolan santai antar agen tiap ~12 menit saat Bos aktif dan bukan jam tidur.
        if n % 24 == 6 {
            let (bed, wake) = sleep_window();
            let m = day_min();
            let night = if bed > wake { m >= bed || m < wake } else { m >= bed && m < wake };
            if !night && active_recently(&c, 10) && !crate::bandit::quiet() {
                let ctx = crate::bandit::ctx("obrolan");
                let a = crate::bandit::choose(&ctx, &["skip", "scene"]);
                crate::bandit::expect(&ctx, &a);
                if a == "scene" {
                    crate::adegan::idle(&tx);
                    tx.send(Event::FeedbackAsk(true));
                }
            }
        }
        // Sapaan proaktif dari pola kebiasaan (jam tidak biasa) dan berita harian; dinilai bandit.
        if n % 10 == 3 && active_recently(&c, 3) && !crate::bandit::quiet() {
            if let Some(msg) = crate::ml::unusual_hour_checkin(&c) {
                let ctx = crate::bandit::ctx("malam");
                let a = crate::bandit::choose(&ctx, &["skip", "scene"]);
                crate::bandit::expect(&ctx, &a);
                if a == "scene" {
                    nudge(&tx, &c, &msg);
                    tx.send(Event::FeedbackAsk(true));
                }
            }
        }
        if n % 20 == 8 && active_recently(&c, 5) && day_min() >= hm(&s.wake) + 30 && !crate::bandit::quiet() && once(&c, &format!("{}news", Local::now().format("%Y-%m-%d"))) {
            let ctx = crate::bandit::ctx("berita");
            let a = crate::bandit::choose(&ctx, &["skip", "scene"]);
            crate::bandit::expect(&ctx, &a);
            if a == "scene" {
                crate::berita::digest(&tx, &http).await;
                tx.send(Event::FeedbackAsk(true));
            }
        }
        crate::bandit::expire_stale();
        if n % 20 == 3 && active_recently(&c, 10) {
            let tx3 = tx.clone();
            std::thread::spawn(move || {
                for m in crate::sistem::auto_tick() {
                    crate::notify::toast("NOVA - otomatis", &m);
                    tx3.notice(m);
                }
            });
        }
        if n % 10 == 5 && active_recently(&c, 10) {
            crate::hewan::tick(&tx);
        }
        if n % 30 == 7 && active_recently(&c, 10) {
            let tx2 = tx.clone();
            std::thread::spawn(move || crate::sistem::suggest(&tx2));
        }
        if n % 10 == 5 && active_recently(&c, 5) {
            crate::musik::check(&tx);
            crate::musik::auto(&tx);
            std::thread::spawn(|| {
                crate::otak::harian(false);
            });
        }
        if n % 60 == 20 && active_recently(&c, 10) && once(&c, &format!("{}dl", Local::now().format("%Y-%m-%d"))) {
            let loose = crate::berkas::loose_count(&dirs::download_dir().unwrap_or_default());
            if loose >= 40 {
                crate::saran::offer(&tx, &format!("Folder Downloads berantakan ({loose} file lepas). Mau kurapikan per jenis? (bisa diurungkan)"), "rapikan folder download", "rapikan");
            }
        }
        if n % 720 == 600 {
            crate::indeks::ensure_fresh();
        }
        if n % 120 == 100 && once(&c, &format!("{}graf", Local::now().format("%Y-%m-%d"))) {
            crate::graf::decay();
        }
        if n % 2 == 1 {
            crate::kurikulum::process_new(&tx).await;
        }
        if n == 4 || n % 120 == 60 {
            crate::bahasa::update(&tx, &http, n == 4).await; // bahasa baru terpasang -> pelajari otomatis
        }
        n += 1;
        tokio::time::sleep(Duration::from_secs(30)).await;
    }
}
