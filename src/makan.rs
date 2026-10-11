//! Tamagotchi-kerja: sahabat NOVA "makan" token AI yang dibakar saat Bos bekerja. Terinspirasi ChanceYu/CoPet dan ntd4996/agentpet
//! (gamifikasi token, XP & level, pencapaian, reaksi saat agen AI berpikir/menulis/error). Sumber token: panggilan model NOVA sendiri
//! (Gemini, OmniRoute, Llama) dan transkrip sesi Claude Code (`~/.claude/projects/**/*.jsonl`, hanya dibaca: angka `usage` saja,
//! isi percakapan tidak disimpan). Riwayat lama tidak dihitung (hanya token baru sejak NOVA berjalan).
use crate::engine::{Event, Tx};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Serialize, Deserialize, Clone, Default)]
struct S {
    kenyang: f32,
    tokens_total: u64,
    per_hari: HashMap<String, u64>,
    offsets: HashMap<String, u64>,
    hari_aktif: Vec<String>,
    unlocked: HashMap<String, i64>,
    chat: u64,
    belai: u64,
    build: u64,
    update_ts: i64,
    lapar_ts: i64,
    xp_jam: (i64, u64),
    suara: Option<bool>,
}

static ST: Mutex<Option<S>> = Mutex::new(None);
static PENDING: Mutex<Vec<String>> = Mutex::new(Vec::new());
static AGEN: Mutex<(String, Option<Instant>)> = Mutex::new((String::new(), None));

fn path() -> std::path::PathBuf {
    crate::config::data_dir().join("makan.json")
}

fn with<R>(f: impl FnOnce(&mut S) -> R) -> R {
    let mut g = ST.lock().unwrap_or_else(|e| e.into_inner());
    let s = g.get_or_insert_with(|| {
        let mut s: S = std::fs::read_to_string(path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        if s.update_ts == 0 {
            s.kenyang = 70.0;
            s.update_ts = chrono::Utc::now().timestamp();
        }
        s
    });
    let now = chrono::Utc::now().timestamp();
    // lapar bertahap: -3 per jam
    s.kenyang = (s.kenyang - (now - s.update_ts) as f32 / 3600.0 * 3.0).clamp(0.0, 100.0);
    s.update_ts = now;
    let before = serde_json::to_string(s).unwrap_or_default();
    let r = f(s);
    let after = serde_json::to_string(s).unwrap_or_default();
    // kenyang turun sedikit tiap panggilan: simpan paling sering tiap ~60 detik kecuali ada perubahan nyata
    static LAST_W: Mutex<Option<Instant>> = Mutex::new(None);
    let due = LAST_W.lock().ok().map_or(true, |g| g.map_or(true, |t| t.elapsed() > Duration::from_secs(60)));
    let big = {
        let (a, b): (S, S) = (serde_json::from_str(&before).unwrap_or_default(), serde_json::from_str(&after).unwrap_or_default());
        (a.tokens_total, a.chat, a.belai, a.build, a.unlocked.len(), a.offsets.len()) != (b.tokens_total, b.chat, b.belai, b.build, b.unlocked.len(), b.offsets.len())
    };
    if big || due {
        if let Ok(mut g) = LAST_W.lock() {
            *g = Some(Instant::now());
        }
        let _ = std::fs::write(path(), after);
    }
    r
}

fn hari() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn tandai_aktif(s: &mut S) {
    let h = hari();
    if !s.hari_aktif.contains(&h) {
        s.hari_aktif.push(h);
        let n = s.hari_aktif.len();
        if n > 60 {
            s.hari_aktif.drain(0..n - 60);
        }
    }
}

/// Beri makan: `tokens` token yang dibakar. Memberi kenyang & XP (maks 60 XP per jam agar tidak diakali).
pub fn feed(tokens: u64) {
    if tokens == 0 {
        return;
    }
    let xp = with(|s| {
        s.tokens_total += tokens;
        *s.per_hari.entry(hari()).or_default() += tokens;
        s.kenyang = (s.kenyang + tokens as f32 / 1000.0).min(100.0);
        tandai_aktif(s);
        let jam = chrono::Utc::now().timestamp() / 3600;
        if s.xp_jam.0 != jam {
            s.xp_jam = (jam, 0);
        }
        let want = (tokens / 2000).max(if tokens >= 300 { 1 } else { 0 });
        let give = want.min(60u64.saturating_sub(s.xp_jam.1));
        s.xp_jam.1 += give;
        give
    });
    if xp > 0 {
        if let Some(m) = crate::buddy::add_xp_amt(xp) {
            PENDING.lock().map(|mut p| p.push(m)).ok();
        }
    }
}

/// Catat kegiatan untuk pencapaian: "obrolan", "belai", "build".
pub fn catat(kind: &str) {
    with(|s| {
        match kind {
            "obrolan" => s.chat += 1,
            "belai" => s.belai += 1,
            "build" => {
                s.build += 1;
                crate::widget::tambah_koin(3);
            }
            _ => {}
        }
        tandai_aktif(s);
    });
}

// ---------------- pencapaian ----------------

struct Ach {
    id: &'static str,
    nama: &'static str,
    desk: &'static str,
}

const ACH: &[Ach] = &[
    Ach { id: "tetas", nama: "Telur Menetas", desk: "Sahabat lahir dan mulai menemani" },
    Ach { id: "lv5", nama: "Mulai Akrab", desk: "Capai Lv.5" },
    Ach { id: "lv10", nama: "Teman Seperjuangan", desk: "Capai Lv.10" },
    Ach { id: "lv25", nama: "Rekan Andal", desk: "Capai Lv.25" },
    Ach { id: "tok10k", nama: "Camilan Token", desk: "Memakan 10 ribu token" },
    Ach { id: "tok100k", nama: "Lahap", desk: "Memakan 100 ribu token" },
    Ach { id: "tok1m", nama: "Pemakan Token", desk: "Memakan 1 juta token" },
    Ach { id: "tok10m", nama: "Gentong Token", desk: "Memakan 10 juta token" },
    Ach { id: "streak3", nama: "Rajin 3 Hari", desk: "Bekerja 3 hari beruntun" },
    Ach { id: "streak7", nama: "Seminggu Penuh", desk: "Bekerja 7 hari beruntun" },
    Ach { id: "chat100", nama: "Ngobrol Terus", desk: "100 pesan ke NOVA" },
    Ach { id: "chat1000", nama: "Sahabat Curhat", desk: "1000 pesan ke NOVA" },
    Ach { id: "belai10", nama: "Disayang", desk: "Membelai sahabat 10 kali" },
    Ach { id: "build1", nama: "Build Pertama", desk: "Satu build berhasil lewat NOVA" },
    Ach { id: "build25", nama: "Tukang Build", desk: "25 build berhasil" },
    Ach { id: "kenyang", nama: "Kenyang Bahagia", desk: "Kenyang 100%" },
    Ach { id: "begadang", nama: "Burung Hantu", desk: "Bekerja lewat tengah malam" },
];

fn streak(s: &S) -> u32 {
    let mut d = chrono::Local::now().date_naive();
    let mut n = 0;
    while s.hari_aktif.contains(&d.format("%Y-%m-%d").to_string()) {
        n += 1;
        d = d.pred_opt().unwrap_or(d);
    }
    n
}

fn terpenuhi(id: &str, s: &S, level: u32) -> bool {
    match id {
        "tetas" => true,
        "lv5" => level >= 5,
        "lv10" => level >= 10,
        "lv25" => level >= 25,
        "tok10k" => s.tokens_total >= 10_000,
        "tok100k" => s.tokens_total >= 100_000,
        "tok1m" => s.tokens_total >= 1_000_000,
        "tok10m" => s.tokens_total >= 10_000_000,
        "streak3" => streak(s) >= 3,
        "streak7" => streak(s) >= 7,
        "chat100" => s.chat >= 100,
        "chat1000" => s.chat >= 1000,
        "belai10" => s.belai >= 10,
        "build1" => s.build >= 1,
        "build25" => s.build >= 25,
        "kenyang" => s.kenyang >= 99.5,
        "begadang" => chrono::Timelike::hour(&chrono::Local::now()) < 4 && !s.hari_aktif.is_empty(),
        _ => false,
    }
}

/// Periksa pencapaian baru; mengembalikan pesan untuk masing-masing yang baru terbuka.
pub fn cek() -> Vec<String> {
    let level = crate::buddy::level_of(crate::buddy::get().xp).0;
    let mut baru = vec![];
    with(|s| {
        let now = chrono::Utc::now().timestamp();
        for a in ACH {
            if !s.unlocked.contains_key(a.id) && terpenuhi(a.id, s, level) {
                s.unlocked.insert(a.id.to_string(), now);
                baru.push(format!("🏆 Pencapaian terbuka: {} — {} (+15 koin)", a.nama, a.desk));
                crate::widget::tambah_koin(15);
            }
        }
    });
    baru
}

pub fn pencapaian() -> String {
    let _ = cek();
    with(|s| {
        let n = s.unlocked.len();
        let mut out = format!("🏆 Pencapaian {n}/{}\n", ACH.len());
        for a in ACH {
            out.push_str(&if let Some(ts) = s.unlocked.get(a.id) {
                format!("✅ {} — {} ({})\n", a.nama, a.desk, chrono::DateTime::from_timestamp(*ts, 0).map(|d| d.with_timezone(&chrono::Local).format("%d/%m").to_string()).unwrap_or_default())
            } else {
                format!("🔒 ??? — {}\n", a.desk)
            });
        }
        out
    })
}

// ---------------- status ----------------

fn bar(v: f32) -> String {
    let f = (v / 10.0).round() as usize;
    format!("{}{}", "█".repeat(f.min(10)), "░".repeat(10 - f.min(10)))
}

pub fn ringkas() -> String {
    with(|s| {
        let hari_ini = s.per_hari.get(&hari()).copied().unwrap_or(0);
        format!("Kenyang [{}] {:.0}% · token dimakan: {} hari ini / {} total · hari beruntun {} · pencapaian {}/{}", bar(s.kenyang), s.kenyang, hari_ini, s.tokens_total, streak(s), s.unlocked.len(), ACH.len())
    })
}

pub fn beri_makan(n: f32) {
    with(|s| s.kenyang = (s.kenyang + n).min(100.0));
}

pub fn kenyang() -> f32 {
    with(|s| s.kenyang)
}

pub fn status_agen() -> String {
    let (st, at) = AGEN.lock().map(|g| g.clone()).unwrap_or_default();
    let segar = at.is_some_and(|t| t.elapsed() < Duration::from_secs(40));
    format!("🤖 Agen AI (Claude Code): {}\n🍖 {}", if segar && !st.is_empty() { st } else { "tidak aktif / idle".into() }, ringkas())
}

// ---------------- suara ----------------

pub fn suara_aktif() -> bool {
    with(|s| s.suara.unwrap_or(true))
}

pub fn set_suara(on: bool) {
    with(|s| s.suara = Some(on));
}

/// Putar efek suara (levelup, achievement, error, blip) tanpa memblokir.
pub fn sfx(nama: &str) {
    if !suara_aktif() {
        return;
    }
    let f = crate::generator::dir_suara().join(format!("{nama}.wav"));
    if !f.exists() {
        crate::generator::bikin_suara();
    }
    if f.exists() {
        let mut c = std::process::Command::new("powershell");
        c.args(["-NoProfile", "-Command", &format!("(New-Object Media.SoundPlayer '{}').PlaySync()", f.display().to_string().replace('\'', "''"))]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            c.creation_flags(0x0800_0000);
        }
        let _ = c.spawn();
    }
}

// ---------------- pemindai transkrip Claude Code + reaksi agen ----------------

fn proj_dir() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude").join("projects")).filter(|p| p.is_dir())
}

fn jsonl_files(dir: &std::path::Path) -> Vec<(std::path::PathBuf, u64, std::time::SystemTime)> {
    let mut out = vec![];
    let cutoff = std::time::SystemTime::now() - Duration::from_secs(2 * 86400);
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            if let Ok(sub) = std::fs::read_dir(e.path()) {
                for f in sub.flatten() {
                    let p = f.path();
                    if p.extension().is_some_and(|x| x == "jsonl") {
                        if let Ok(m) = f.metadata() {
                            if m.modified().map(|t| t > cutoff).unwrap_or(false) {
                                out.push((p, m.len(), m.modified().unwrap_or(std::time::UNIX_EPOCH)));
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

/// Jalankan pemindai di latar: beri makan dari token baru, deteksi keadaan agen, kirim notifikasi/reaksi.
pub fn start(tx: Tx) {
    std::thread::spawn(move || {
        let mut seen_ids: VecDeque<String> = VecDeque::new();
        let mut seen_set: HashSet<String> = HashSet::new();
        let mut last_state = String::new();
        let mut last_error_note = Instant::now() - Duration::from_secs(600);
        let mut first = true;
        loop {
            std::thread::sleep(Duration::from_secs(4));
            // pesan tertunda (naik level dari panggilan model)
            let pend: Vec<String> = PENDING.lock().map(|mut p| std::mem::take(&mut *p)).unwrap_or_default();
            for m in pend {
                crate::notify::toast("NOVA - naik level", &m);
                tx.notice(m);
                tx.send(Event::Mood { emo: "gembira".into(), level: 1.0 });
                sfx("levelup");
            }
            for m in cek() {
                crate::notify::toast("NOVA - pencapaian", &m);
                tx.notice(m);
                tx.send(Event::Mood { emo: "gembira".into(), level: 1.0 });
                sfx("achievement");
            }
            if with(|s| s.kenyang < 15.0 && chrono::Utc::now().timestamp() - s.lapar_ts > 3 * 3600) {
                with(|s| s.lapar_ts = chrono::Utc::now().timestamp());
                tx.notice("🍖 Sahabat lapar token… ayo bekerja/ngoding bersama agen AI supaya dia kenyang.".to_string());
                tx.send(Event::Mood { emo: "lelah".into(), level: 0.7 });
            }
            let Some(dir) = proj_dir() else { continue };
            let files = jsonl_files(&dir);
            let mut newest: Option<(std::time::SystemTime, String)> = None;
            for (p, len, mt) in files {
                let key = p.to_string_lossy().to_string();
                let off = with(|s| *s.offsets.entry(key.clone()).or_insert(if first { len } else { 0 }));
                let off = if len < off { 0 } else { off };
                if len == off {
                    continue;
                }
                use std::io::{Read, Seek, SeekFrom};
                let Ok(mut f) = std::fs::File::open(&p) else { continue };
                if f.seek(SeekFrom::Start(off)).is_err() {
                    continue;
                }
                let mut buf = String::new();
                if f.take(len - off).read_to_string(&mut buf).is_err() {
                    continue;
                }
                // hanya proses baris lengkap
                let complete = buf.rfind('\n').map(|i| i + 1).unwrap_or(0);
                let mut tok = 0u64;
                let mut state = String::new();
                for line in buf[..complete].lines() {
                    let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
                    let m = &v["message"];
                    if let Some(u) = m["usage"].as_object() {
                        let id = m["id"].as_str().unwrap_or("").to_string();
                        if id.is_empty() || seen_set.insert(id.clone()) {
                            seen_ids.push_back(id);
                            if seen_ids.len() > 800 {
                                if let Some(o) = seen_ids.pop_front() {
                                    seen_set.remove(&o);
                                }
                            }
                            let g = |k: &str| u.get(k).and_then(|x| x.as_u64()).unwrap_or(0);
                            tok += g("input_tokens") + g("output_tokens") + g("cache_creation_input_tokens");
                        }
                    }
                    if let Some(arr) = m["content"].as_array() {
                        for c in arr {
                            match c["type"].as_str().unwrap_or("") {
                                "tool_use" => {
                                    state = match c["name"].as_str().unwrap_or("") {
                                        "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => "menulis kode".to_string(),
                                        "Bash" | "PowerShell" => "menjalankan perintah".to_string(),
                                        "Read" | "Grep" | "Glob" => "membaca kode".to_string(),
                                        _ => "memakai alat".to_string(),
                                    }
                                }
                                "text" if v["type"] == "assistant" => state = "berpikir/menjawab".to_string(),
                                "tool_result" if c["is_error"].as_bool().unwrap_or(false) => state = "mengalami error".to_string(),
                                _ => {}
                            }
                        }
                    }
                }
                with(|s| {
                    s.offsets.insert(key.clone(), off + complete as u64);
                });
                if !first && tok > 0 {
                    feed(tok);
                }
                if !state.is_empty() && newest.as_ref().map_or(true, |(t, _)| mt >= *t) {
                    newest = Some((mt, state));
                }
            }
            first = false;
            if let Some((_, st)) = newest {
                if let Ok(mut g) = AGEN.lock() {
                    *g = (st.clone(), Some(Instant::now()));
                }
                if st != last_state {
                    last_state = st.clone();
                    match st.as_str() {
                        "menulis kode" => tx.send(Event::Act { kind: "ketik".into(), text: String::new(), ms: 5000 }),
                        "berpikir/menjawab" => tx.send(Event::Act { kind: "pikir".into(), text: String::new(), ms: 4000 }),
                        "menjalankan perintah" => tx.send(Event::Act { kind: "palu".into(), text: String::new(), ms: 4000 }),
                        "membaca kode" => tx.send(Event::Act { kind: "baca".into(), text: String::new(), ms: 3000 }),
                        "mengalami error" if last_error_note.elapsed() > Duration::from_secs(60) => {
                            last_error_note = Instant::now();
                            tx.send(Event::Mood { emo: "pusing".into(), level: 0.9 });
                            tx.notice("⚠ Agen AI mengalami error — mau kubantu cek? Ketik 'jelaskan error' bila ada log build.".to_string());
                            sfx("error");
                        }
                        _ => {}
                    }
                }
            } else if last_state != "" {
                let idle = AGEN.lock().map(|g| g.1.map_or(true, |t| t.elapsed() > Duration::from_secs(40))).unwrap_or(true);
                if idle {
                    last_state.clear();
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn achievements_logic() {
        let mut s = S::default();
        assert!(terpenuhi("tetas", &s, 1));
        assert!(!terpenuhi("tok10k", &s, 1));
        s.tokens_total = 12_000;
        s.hari_aktif = (0..3).map(|i| (chrono::Local::now().date_naive() - chrono::Duration::days(i)).format("%Y-%m-%d").to_string()).collect();
        assert!(terpenuhi("tok10k", &s, 1) && terpenuhi("streak3", &s, 1) && !terpenuhi("streak7", &s, 1));
        assert!(terpenuhi("lv5", &s, 5));
    }
}
