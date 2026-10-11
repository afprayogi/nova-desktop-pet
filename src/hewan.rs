//! Otak proaktif peliharaan: pola jam aktif harian, prediksi kelelahan, memori jangka pendek (aktivitas beberapa jam terakhir),
//! dan perencana bertujuan (jaga kesehatan + bantu produktivitas). Sebelum mengganggu, agen merefleksikan apakah saatnya tepat.
//! Tiap pop-up membawa tombol aksi cepat (Ajak Main, Beri Perhatian, Tunda, Tutup).
use crate::engine::{Event, Tx};
use chrono::{Local, TimeZone, Timelike};
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

struct St {
    last_touch: Instant,
    last_pop: Instant,
    last_water: Instant,
    last_break: Instant,
    snooze_until: Instant,
    open: bool,
    kind: &'static str,
    ignored: u32,
    refused_break: u32,
    milestone: i64,
    /// memori jangka pendek: (waktu, aplikasi) per pengamatan
    mem: VecDeque<(chrono::DateTime<Local>, String)>,
    events: VecDeque<(chrono::DateTime<Local>, String)>,
}

static ST: Mutex<Option<St>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut St) -> R) -> R {
    let mut g = ST.lock().unwrap_or_else(|e| e.into_inner());
    let now = Instant::now();
    let s = g.get_or_insert_with(|| St {
        last_touch: now,
        last_pop: now,
        last_water: now,
        last_break: now,
        snooze_until: now,
        open: false,
        kind: "",
        ignored: 0,
        refused_break: 0,
        milestone: -1,
        mem: VecDeque::new(),
        events: VecDeque::new(),
    });
    f(s)
}

/// Dipanggil setiap Bos berinteraksi.
pub fn touch() {
    with(|s| s.last_touch = Instant::now());
}

fn examples() -> i64 {
    crate::teknik::open().ok().and_then(|c| c.query_row("SELECT COUNT(*) FROM intent_ex", [], |r| r.get(0)).ok()).unwrap_or(0)
}

// ---------------- pola jam aktif & prediksi kelelahan ----------------

pub struct Pola {
    /// rata-rata menit aktif per jam (0..24) selama 14 hari terakhir
    pub per_jam: [f32; 24],
    /// panjang sesi aktif tipikal (menit) sebelum Bos biasanya berhenti
    pub sesi_tipikal: u32,
    pub hari: usize,
}

pub fn pola() -> Pola {
    let mut per = [0f32; 24];
    let mut runs: Vec<u32> = vec![];
    let mut days = std::collections::HashSet::new();
    if let Ok(c) = crate::assistant::db() {
        let from = chrono::Utc::now().timestamp() / 60 - 14 * 1440;
        if let Ok(mut st) = c.prepare("SELECT min FROM act WHERE min >= ?1 ORDER BY min") {
            let mins: Vec<i64> = st.query_map([from], |r| r.get(0)).map(|r| r.flatten().collect()).unwrap_or_default();
            let mut run: Option<(i64, i64)> = None; // (awal, terakhir)
            for m in &mins {
                if let Some(d) = Local.timestamp_opt(m * 60, 0).single() {
                    per[d.hour() as usize] += 1.0;
                    days.insert(d.date_naive());
                }
                match run {
                    Some((s, l)) if m - l <= 5 => run = Some((s, *m)),
                    Some((s, l)) => {
                        runs.push((l - s) as u32);
                        run = Some((*m, *m));
                    }
                    None => run = Some((*m, *m)),
                }
            }
            if let Some((s, l)) = run {
                runs.push((l - s) as u32);
            }
        }
    }
    let n = days.len().max(1) as f32;
    for v in per.iter_mut() {
        *v /= n;
    }
    runs.retain(|r| *r >= 15);
    runs.sort_unstable();
    let sesi = if runs.len() >= 3 { runs[runs.len() * 3 / 4] } else { 0 };
    Pola { per_jam: per, sesi_tipikal: sesi, hari: days.len() }
}

/// Skor kelelahan 0..1 dari durasi aktif beruntun, kebiasaan sesi, dan jam malam.
pub fn kelelahan(p: &Pola, streak: u32) -> f32 {
    let batas = if p.sesi_tipikal >= 30 { p.sesi_tipikal as f32 } else { 90.0 };
    let mut f = streak as f32 / batas.max(30.0);
    let h = Local::now().hour();
    if h >= 23 || h < 4 {
        f += 0.25;
    }
    let rule = f.clamp(0.0, 1.5) / 1.5;
    match crate::belajar::p_break(streak) {
        Some(p) => 0.5 * rule + 0.5 * p, // aturan + jaringan saraf yang dilatih dari kebiasaan Bos
        None => rule,
    }
}

fn prediksi(p: &Pola, streak: u32) -> Option<u32> {
    let batas = if p.sesi_tipikal >= 30 { p.sesi_tipikal } else { 90 };
    (streak > 0).then(|| batas.saturating_sub(streak))
}

/// Ringkasan untuk perintah /pola.
pub fn laporan() -> String {
    let p = pola();
    let streak = crate::assistant::streak_min();
    let mut idx: Vec<usize> = (0..24).collect();
    idx.sort_by(|a, b| p.per_jam[*b].total_cmp(&p.per_jam[*a]));
    let top: Vec<String> = idx.iter().take(3).filter(|i| p.per_jam[**i] > 1.0).map(|i| format!("{:02}:00 (~{:.0} mnt/jam)", i, p.per_jam[*i])).collect();
    let bar: String = (5..24)
        .map(|h| {
            let v = p.per_jam[h];
            if v >= 45.0 {
                '9'
            } else if v >= 30.0 {
                '6'
            } else if v >= 15.0 {
                '4'
            } else if v >= 3.0 {
                '2'
            } else {
                '.'
            }
        })
        .collect();
    let f = kelelahan(&p, streak);
    let mut t = format!("Pola jam aktif ({} hari data), jam 05→23:\n{bar}\n", p.hari);
    t.push_str(&if top.is_empty() { "Data belum cukup untuk jam puncak.\n".to_string() } else { format!("Paling produktif: {}\n", top.join(", ")) });
    t.push_str(&format!("Sesi aktif tipikal: {} menit · sekarang: {streak} menit\n", if p.sesi_tipikal > 0 { p.sesi_tipikal.to_string() } else { "belum terpelajari (pakai 90)".into() }));
    t.push_str(&format!("Tingkat kelelahan: {:.0}%", f * 100.0));
    if let Some(r) = prediksi(&p, streak) {
        t.push_str(&if r == 0 { " — sudah waktunya istirahat.".to_string() } else { format!(" — diprediksi butuh istirahat ±{r} menit lagi.") });
    }
    t.push_str(&format!("\n{}\n{}", crate::belajar::info(), memori()));
    t
}

// ---------------- memori jangka pendek ----------------

fn rekam(app: String) {
    with(|s| {
        let now = Local::now();
        if !app.is_empty() {
            s.mem.push_back((now, app));
        }
        while s.mem.front().is_some_and(|(t, _)| now.signed_duration_since(*t).num_minutes() > 240) {
            s.mem.pop_front();
        }
        while s.events.front().is_some_and(|(t, _)| now.signed_duration_since(*t).num_minutes() > 240) {
            s.events.pop_front();
        }
    });
}

fn catat(e: &str) {
    with(|s| s.events.push_back((Local::now(), e.to_string())));
}

/// Konteks aktivitas jam terakhir (dipakai juga sebagai konteks untuk Gemini).
pub fn memori() -> String {
    with(|s| {
        let now = Local::now();
        let mut cnt: std::collections::HashMap<&str, u32> = Default::default();
        for (_, a) in &s.mem {
            *cnt.entry(a.as_str()).or_default() += 1;
        }
        let mut v: Vec<_> = cnt.into_iter().collect();
        v.sort_by_key(|x| std::cmp::Reverse(x.1));
        let apps: Vec<String> = v.iter().take(3).map(|(a, n)| format!("{a} (~{} mnt)", n * 2)).collect();
        let mut t = if apps.is_empty() { "Memori singkat: belum ada aktivitas terpantau.".to_string() } else { format!("Memori singkat (beberapa jam terakhir): sedang fokus di {}.", apps.join(", ")) };
        for (at, e) in s.events.iter().rev().take(3) {
            t.push_str(&format!(" {} menit lalu: {e}.", now.signed_duration_since(*at).num_minutes()));
        }
        let f = crate::fakta::ringkas();
        if !f.is_empty() {
            t.push_str(&format!(" Fakta tentang Bos: {f}."));
        }
        t
    })
}

// ---------------- perencana bertujuan ----------------

enum Aksi {
    Istirahat(String),
    Air,
    Tidur(String),
    Milestone(String),
    Mood(String),
    Sapa(String),
    Tips,
}

/// Perencana: dua tujuan (kesehatan + produktivitas) -> pilih satu langkah kecil paling bernilai sekarang.
fn rencanakan(streak: u32) -> Option<Aksi> {
    let h = Local::now().hour();
    let p = pola();
    let f = kelelahan(&p, streak);
    let n = examples();
    let (idle, since_water, since_break, refused, ms) = with(|s| (s.last_touch.elapsed().as_secs() / 60, s.last_water.elapsed().as_secs() / 60, s.last_break.elapsed().as_secs() / 60, s.refused_break, s.milestone));
    if f >= 0.6 && since_break >= if refused > 0 { 20 } else { 5 } {
        let rest = prediksi(&p, streak).unwrap_or(0);
        let txt = if refused >= 2 {
            format!("Bos… sudah {streak} menit dan tadi aku diabaikan dua kali 🥺 Lima menit saja, ya? Kesehatanmu lebih penting dari tugas ini.")
        } else {
            format!("Kamu sudah {streak} menit non-stop (biasanya berhenti ±{} menit){}. Yuk istirahat 5 menit ☕", if p.sesi_tipikal > 0 { p.sesi_tipikal } else { 90 }, if rest == 0 { "" } else { " — kelelahan mulai naik" })
        };
        return Some(Aksi::Istirahat(txt));
    }
    if (h >= 23 || h < 4) && streak > 0 {
        return Some(Aksi::Tidur("Sudah larut, Bos. Ketik 'bisa tidur 8 jam?' untuk cek sisa waktu tidurmu 🌙".into()));
    }
    if ms >= 0 && n / 50 > ms {
        return Some(Aksi::Milestone(format!("Level up! Aku sudah belajar dari {n} contoh kalimatmu — makin paham gayamu 🎉")));
    }
    if idle >= 60 {
        return Some(Aksi::Mood("Aku mulai ngantuk nih… sudah lama sepi. Main sebentar?".into()));
    }
    if since_water >= 60 && streak > 0 {
        return Some(Aksi::Air);
    }
    let top = (0..24).max_by(|a, b| p.per_jam[*a].total_cmp(&p.per_jam[*b])).unwrap_or(10);
    if p.hari >= 3 && h as usize == top && streak < 30 {
        return Some(Aksi::Sapa(format!("Jam {top:02}:00 biasanya waktu paling produktifmu. Tentukan 1 tugas utama sekarang ✨")));
    }
    let g = match h {
        4..=10 => "Pagi, Bos! Semoga harimu lancar ☀",
        11..=14 => "Siang, Bos. Jangan lupa makan 🍚",
        15..=17 => "Sore, Bos — tinggal sedikit lagi 💪",
        _ => "Malam, Bos. Jangan lupa batas tidurmu 🌙",
    };
    let quiet_for = with(|s| s.last_pop.elapsed().as_secs());
    if quiet_for > 3 * 3600 {
        return Some(Aksi::Sapa(g.into()));
    }
    (quiet_for > 2 * 3600).then_some(Aksi::Tips)
}

/// Dipanggil berkala saat Bos aktif. Paling banyak satu pop-up per 25 menit.
pub fn tick(tx: &Tx) {
    rekam(crate::konteks::current_app());
    if crate::bandit::quiet() || crate::buddy::diam() {
        return;
    }
    // Refleksi diri: jangan ganggu saat mengetik/menonton (tunda 10 menit).
    if crate::ambien::typing() || crate::ambien::media() == "video" {
        with(|s| s.snooze_until = s.snooze_until.max(Instant::now() + Duration::from_secs(10 * 60)));
        return;
    }
    // pop-up yang tidak dijawab 10 menit dianggap diabaikan
    if with(|s| s.open && s.last_pop.elapsed() > Duration::from_secs(10 * 60)) {
        let (kind, ign) = with(|s| {
            s.open = false;
            s.ignored += 1;
            if s.kind == "istirahat" {
                s.refused_break += 1;
            }
            (s.kind, s.ignored)
        });
        catat(&format!("pop-up {kind} diabaikan"));
        if ign >= 3 {
            tx.send(Event::Mood { emo: "menangis".into(), level: 0.9 });
        }
        return;
    }
    let ok = with(|s| {
        if s.milestone < 0 {
            s.milestone = examples() / 50;
        }
        !s.open && Instant::now() >= s.snooze_until && s.last_pop.elapsed() >= Duration::from_secs(25 * 60)
    });
    if !ok {
        return;
    }
    let streak = crate::assistant::streak_min();
    let Some(a) = rencanakan(streak) else { return };
    let (text, kind) = match a {
        Aksi::Istirahat(t) => (t, "istirahat"),
        Aksi::Air => ("Sudah minum air? Segelas dulu, yuk 💧".to_string(), "air"),
        Aksi::Tidur(t) => (t, "tidur"),
        Aksi::Milestone(t) => {
            with(|s| s.milestone = examples() / 50);
            (t, "milestone")
        }
        Aksi::Mood(t) => (t, "mood"),
        Aksi::Sapa(t) => (t, "sapa"),
        Aksi::Tips => ("Coba: 'cek wifi', 'diskusi <topik>', '/pola', atau seret berkas ke aku ✨".to_string(), "tips"),
    };
    with(|s| {
        s.last_pop = Instant::now();
        s.open = true;
        s.kind = kind;
    });
    if kind == "istirahat" && with(|s| s.refused_break) >= 2 {
        tx.send(Event::Mood { emo: "menangis".into(), level: 0.8 });
    } else if kind == "mood" {
        tx.send(Event::Mood { emo: "lelah".into(), level: 0.7 });
    }
    tx.send(Event::Ask { question: text, options: vec!["Ajak Main".into(), "Beri Perhatian".into(), "Tunda".into(), "Tutup".into()] });
}

/// Jawaban tombol pop-up. Mengembalikan balasan hangat bila tombol itu milik pop-up peliharaan.
pub fn resolve(answer: &str, tx: &Tx) -> Option<String> {
    let a = answer.trim().to_lowercase();
    if !with(|s| s.open) || !["ajak main", "beri perhatian", "tunda", "tutup"].contains(&a.as_str()) {
        return None;
    }
    let kind = with(|s| s.kind);
    with(|s| {
        s.open = false;
        s.last_touch = Instant::now();
        match a.as_str() {
            "tunda" => s.snooze_until = Instant::now() + Duration::from_secs(30 * 60),
            "tutup" => {
                s.ignored += 1;
                if kind == "istirahat" {
                    s.refused_break += 1;
                }
            }
            _ => s.ignored = 0,
        }
        if kind == "air" && a != "tutup" {
            s.last_water = Instant::now();
        }
        if kind == "istirahat" && a != "tutup" && a != "tunda" {
            s.last_break = Instant::now();
            s.refused_break = 0;
        }
    });
    catat(&format!("pop-up {kind}: Bos memilih '{a}'"));
    if kind == "istirahat" && a != "tutup" && a != "tunda" {
        if let Some(m) = crate::buddy::add_xp("istirahat") {
            tx.notice(m);
        }
    }
    if a == "tutup" && with(|s| s.ignored) >= 3 {
        tx.send(Event::Mood { emo: "menangis".into(), level: 0.9 });
        return Some("Hiks… baiklah Bos 😢 Aku akan lebih jarang mengganggu.".into());
    }
    Some(match a.as_str() {
        "ajak main" => {
            tx.send(Event::Act { kind: "tawa".into(), text: String::new(), ms: 4000 });
            tx.send(Event::Mood { emo: "gembira".into(), level: 0.9 });
            "Yeay, main! 🎉 Aku senang ditemani Bos.".into()
        }
        "beri perhatian" => {
            tx.send(Event::Mood { emo: "gembira".into(), level: 0.9 });
            if kind == "istirahat" {
                "Terima kasih sudah mau istirahat, Bos 🥰 Aku catat ya.".into()
            } else {
                "Terima kasih, Bos 🥰 Energiku penuh lagi!".into()
            }
        }
        "tunda" => "Oke, kuingatkan 30 menit lagi ya ⏰".into(),
        _ => "Siap, kututup. Panggil aku kapan saja 🙂".into(),
    })
}

// ---------------- istirahat paksa: kelelahan > 80% -> laptop tidur ----------------

const BATAS: f32 = 0.80;
const HITUNG_MUNDUR: u64 = 90;

struct Guard {
    pending_until: Option<Instant>,
    snooze_until: Instant,
    snoozes: u32,
    cooldown_until: Instant,
}

static GUARD: Mutex<Option<Guard>> = Mutex::new(None);

fn with_g<R>(f: impl FnOnce(&mut Guard) -> R) -> R {
    let mut g = GUARD.lock().unwrap_or_else(|e| e.into_inner());
    let now = Instant::now();
    f(g.get_or_insert_with(|| Guard { pending_until: None, snooze_until: now, snoozes: 0, cooldown_until: now }))
}

fn flag_path() -> std::path::PathBuf {
    crate::config::data_dir().join("istirahat_paksa.txt")
}

/// Aktif secara bawaan; `/paksa off` mematikan.
pub fn paksa_aktif() -> bool {
    std::fs::read_to_string(flag_path()).map(|t| t.trim() != "off").unwrap_or(true)
}

pub fn paksa_cmd(arg: &str) -> String {
    match arg.trim() {
        "off" => {
            let _ = std::fs::write(flag_path(), "off");
            "Istirahat paksa DIMATIKAN. Laptop tidak akan ditidurkan otomatis.".into()
        }
        "on" => {
            let _ = std::fs::write(flag_path(), "on");
            "Istirahat paksa AKTIF: bila kelelahan di atas 80%, kuberi hitung mundur 90 detik lalu laptop ditidurkan (bisa ditunda 15 menit, maksimal 2 kali per sesi).".into()
        }
        _ => format!("Istirahat paksa: {}. Ubah dengan /paksa on|off. Batas kelelahan {:.0}%, hitung mundur {HITUNG_MUNDUR} detik, tunda 15 menit (maks 2x).", if paksa_aktif() { "AKTIF" } else { "mati" }, BATAS * 100.0),
    }
}

/// Sibuk = CPU tinggi, video diputar, atau proses berat (build/salin/enkode) berjalan: jangan tidurkan laptop.
fn sibuk(sys: &mut sysinfo::System) -> bool {
    sys.refresh_cpu_usage();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    sys.global_cpu_usage() > 70.0
        || crate::ambien::media() == "video"
        || sys.processes().values().any(|p| {
            let n = p.name().to_string_lossy().to_lowercase();
            ["cargo", "rustc", "robocopy", "xcopy", "msbuild", "ffmpeg", "handbrake", "7z"].iter().any(|k| n.contains(k))
        })
}

fn tidurkan() {
    let mut c = std::process::Command::new("powershell");
    c.args(["-NoProfile", "-Command", "Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.Application]::SetSuspendState('Suspend', $false, $false)"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let _ = c.spawn();
}

/// Jawaban tombol pada peringatan istirahat paksa.
pub fn resolve_paksa(answer: &str) -> Option<String> {
    let a = answer.trim().to_lowercase();
    if with_g(|g| g.pending_until.is_none()) {
        return None;
    }
    match a.as_str() {
        "tidur sekarang" => {
            with_g(|g| {
                g.pending_until = None;
                g.cooldown_until = Instant::now() + Duration::from_secs(30 * 60);
            });
            tidurkan();
            Some("Selamat beristirahat, Bos 😴".into())
        }
        "tunda 1 jam" => {
            with_g(|g| {
                g.pending_until = None;
                g.snooze_until = Instant::now() + Duration::from_secs(3600);
            });
            Some("Siap, kutunda 1 jam. Jangan lupa istirahat ya 🙏".into())
        }
        "tunda 15 menit" => {
            let ok = with_g(|g| {
                if g.snoozes >= 2 {
                    return false;
                }
                g.snoozes += 1;
                g.pending_until = None;
                g.snooze_until = Instant::now() + Duration::from_secs(15 * 60);
                true
            });
            Some(if ok { "Baik, 15 menit lagi ya. Setelah itu aku tidak bisa menunda lagi ⏰".into() } else { "Sudah dua kali ditunda; saatnya istirahat sungguhan, Bos.".into() })
        }
        _ => None,
    }
}

/// Pengawas: dijalankan di thread terpisah; memeriksa tiap 20 detik. Mengirim peringatan lalu menidurkan laptop.
pub fn start_guard(tx: Tx) {
    std::thread::spawn(move || {
        let mut pola_cache: Option<(Instant, Pola)> = None;
        let mut sys = sysinfo::System::new();
        loop {
            std::thread::sleep(Duration::from_secs(20));
            if !paksa_aktif() {
                with_g(|g| g.pending_until = None);
                continue;
            }
            let streak = crate::assistant::streak_min();
            if streak == 0 {
                with_g(|g| {
                    g.snoozes = 0; // sesi baru setelah istirahat
                    g.pending_until = None;
                });
                continue;
            }
            if pola_cache.as_ref().map_or(true, |(t, _)| t.elapsed() > Duration::from_secs(300)) {
                pola_cache = Some((Instant::now(), pola()));
            }
            let f = kelelahan(&pola_cache.as_ref().unwrap().1, streak);
            let now = Instant::now();
            if f > BATAS && sibuk(&mut sys) {
                with_g(|g| g.pending_until = None);
                continue;
            }
            let (due, start) = with_g(|g| {
                if f <= BATAS || now < g.cooldown_until || now < g.snooze_until {
                    if f <= BATAS {
                        g.pending_until = None;
                    }
                    return (false, false);
                }
                match g.pending_until {
                    None => {
                        g.pending_until = Some(now + Duration::from_secs(HITUNG_MUNDUR));
                        (false, true)
                    }
                    Some(t) => (now >= t, false),
                }
            });
            if start {
                let msg = format!("Kelelahan {:.0}% (sudah {streak} menit non-stop). Laptop akan ditidurkan dalam {HITUNG_MUNDUR} detik — simpan pekerjaanmu!", f * 100.0);
                crate::notify::toast("NOVA - waktunya istirahat", &msg);
                tx.send(Event::Mood { emo: "lelah".into(), level: 0.9 });
                tx.send(Event::Ask { question: msg, options: vec!["Tidur sekarang".into(), "Tunda 15 menit".into(), "Tunda 1 jam".into()] });
            }
            if due {
                with_g(|g| {
                    g.pending_until = None;
                    g.cooldown_until = Instant::now() + Duration::from_secs(30 * 60);
                });
                tx.notice("😴 Laptop ditidurkan karena kelelahan di atas 80%.");
                tidurkan();
            }
        }
    });
}
