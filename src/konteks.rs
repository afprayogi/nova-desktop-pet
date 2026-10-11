//! Kesadaran konteks: aplikasi aktif -> "mode" (Coding, Teknik, Dokumen, ...), status idle, dan kalender .ics lokal.
//! Privasi: hanya NAMA proses aktif yang dipakai (tidak ada judul jendela/ketikan); semuanya di RAM.
use chrono::{DateTime, Local, NaiveDateTime, TimeZone, Utc};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Instant, SystemTime};

/// Detik sejak input terakhir (mouse/keyboard) - hanya angka, bukan isinya.
pub static IDLE_SECS: AtomicU64 = AtomicU64::new(0);
static APP: Mutex<String> = Mutex::new(String::new());
static LAST_MODE: Mutex<String> = Mutex::new(String::new());
static HINTED: Mutex<Option<HashMap<String, Instant>>> = Mutex::new(None);

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    Coding,
    Teknik,
    Dokumen,
    Browsing,
    Media,
    Chat,
    Lain,
}

impl Mode {
    pub fn label(&self) -> &'static str {
        match self {
            Mode::Coding => "Mode Coding",
            Mode::Teknik => "Mode Teknik",
            Mode::Dokumen => "Mode Dokumen",
            Mode::Browsing => "Mode Browsing",
            Mode::Media => "Mode Media",
            Mode::Chat => "Mode Chat",
            Mode::Lain => "Santai",
        }
    }
    /// Agen yang paling cocok tampil: 0 NOVA, 1 VOLT, 2 SENTRY, 3 MEMO.
    pub fn agent(&self) -> usize {
        let o = OVERRIDE.load(std::sync::atomic::Ordering::Relaxed);
        if o < 4 {
            return o;
        }
        match self {
            Mode::Coding | Mode::Teknik => 1,
            Mode::Dokumen => 3,
            _ => 0,
        }
    }
}

pub fn classify(app: &str) -> Mode {
    let a = app.to_lowercase();
    let has = |ks: &[&str]| ks.iter().any(|k| a.contains(k));
    if has(&["acad", "eplan", "matlab", "simulink", "codesys", "labview", "multisim", "proteus", "kicad", "pcbnew", "ltspice", "xvii", "tia", "step7", "wincc", "cade", "etap", "digsilent", "arduino"]) {
        Mode::Teknik
    } else if has(&["code", "devenv", "idea", "pycharm", "clion", "rustrover", "sublime", "notepad++", "cursor", "windowsterminal", "wt", "cmd", "powershell", "pwsh", "mintty", "studio64", "webstorm"]) {
        Mode::Coding
    } else if has(&["winword", "excel", "powerpnt", "acrobat", "acrord", "sumatra", "onenote", "foxit", "soffice", "wps"]) {
        Mode::Dokumen
    } else if has(&["chrome", "msedge", "firefox", "brave", "opera", "vivaldi"]) {
        Mode::Browsing
    } else if has(&["vlc", "spotify", "wmplayer", "musicbee", "potplayer", "netflix"]) {
        Mode::Media
    } else if has(&["whatsapp", "telegram", "discord", "slack", "teams", "zoom", "signal"]) {
        Mode::Chat
    } else {
        Mode::Lain
    }
}

pub fn set_app(app: &str, idle: u64) {
    IDLE_SECS.store(idle, Ordering::Relaxed);
    if let Ok(mut a) = APP.lock() {
        *a = app.to_string();
    }
}

pub fn current_app() -> String {
    APP.lock().map(|a| a.clone()).unwrap_or_default()
}

pub fn current() -> (Mode, u64) {
    let app = APP.lock().map(|a| a.clone()).unwrap_or_default();
    (classify(&app), IDLE_SECS.load(Ordering::Relaxed))
}

/// Saran sekali-per-30-menit saat pindah ke mode yang relevan: (agen, teks).
pub fn mode_hint() -> Option<(usize, String)> {
    let (m, idle) = current();
    if idle > 120 {
        return None;
    }
    let label = m.label().to_string();
    {
        let mut l = LAST_MODE.lock().ok()?;
        if *l == label {
            return None;
        }
        *l = label.clone();
    }
    let text = match m {
        Mode::Coding => "Mode Coding aktif. Kalau ketemu error, ketik: tanya <pesan error> atau minta aku 'baca layar'.",
        Mode::Teknik => "Mode Teknik aktif. Butuh hitungan? Ketik: hitung drop tegangan ... atau tanya <topik>.",
        Mode::Dokumen => "Mode Dokumen aktif. Mau kuringkas atau kupelajari PDF-nya? Ketik: ringkas pdf <path> / pelajari <path>.",
        _ => return None,
    };
    let mut g = HINTED.lock().ok()?;
    let map = g.get_or_insert_with(HashMap::new);
    if map.get(&label).is_some_and(|t| t.elapsed().as_secs() < 1800) {
        return None;
    }
    map.insert(label, Instant::now());
    Some((m.agent(), text.to_string()))
}

// ---------- Kalender (.ics lokal) ----------

#[derive(Clone)]
pub struct Ev {
    pub start: DateTime<Local>,
    pub title: String,
}

static ICS: Mutex<Option<(SystemTime, Vec<Ev>)>> = Mutex::new(None);
static GOOGLE: Mutex<Vec<Ev>> = Mutex::new(Vec::new());
static OUTLOOK: Mutex<Vec<Ev>> = Mutex::new(Vec::new());

pub fn set_outlook_events(v: Vec<Ev>) {
    if let Ok(mut g) = OUTLOOK.lock() {
        *g = v;
    }
}

pub fn set_google_events(v: Vec<Ev>) {
    if let Ok(mut g) = GOOGLE.lock() {
        *g = v;
    }
}

fn parse_dt(v: &str) -> Option<DateTime<Local>> {
    let v = v.trim();
    if v.len() < 15 {
        return None; // all-day (VALUE=DATE) dilewati
    }
    let utc = v.ends_with('Z');
    let n = NaiveDateTime::parse_from_str(v.trim_end_matches('Z'), "%Y%m%dT%H%M%S").ok()?;
    if utc {
        Some(Utc.from_utc_datetime(&n).with_timezone(&Local))
    } else {
        Local.from_local_datetime(&n).single()
    }
}

pub fn parse_ics(text: &str) -> Vec<Ev> {
    // Gabungkan baris lanjutan (diawali spasi/tab).
    let mut lines: Vec<String> = vec![];
    for l in text.lines() {
        if (l.starts_with(' ') || l.starts_with('\t')) && !lines.is_empty() {
            lines.last_mut().unwrap().push_str(l.trim_start());
        } else {
            lines.push(l.trim_end().to_string());
        }
    }
    let (mut out, mut inside, mut start, mut title) = (vec![], false, None, String::new());
    for l in lines {
        if l.starts_with("BEGIN:VEVENT") {
            (inside, start, title) = (true, None, String::new());
        } else if l.starts_with("END:VEVENT") {
            if let (true, Some(s)) = (inside, start) {
                out.push(Ev { start: s, title: if title.is_empty() { "(tanpa judul)".into() } else { title.clone() } });
            }
            inside = false;
        } else if inside {
            if let Some((k, v)) = l.split_once(':') {
                let key = k.split(';').next().unwrap_or("");
                match key {
                    "DTSTART" => start = parse_dt(v),
                    "SUMMARY" => title = v.replace("\\,", ",").replace("\\;", ";").chars().take(80).collect(),
                    _ => {}
                }
            }
        }
    }
    out.sort_by_key(|e| e.start);
    out
}

/// Acara dari `kalender.ics` di folder data (dimuat ulang bila file berubah). Pengulangan (RRULE) belum didukung.
pub fn events() -> Vec<Ev> {
    let mut all = ics_events();
    all.extend(GOOGLE.lock().map(|g| g.clone()).unwrap_or_default());
    all.extend(OUTLOOK.lock().map(|g| g.clone()).unwrap_or_default());
    all.sort_by_key(|e| e.start);
    all.dedup_by(|a, b| a.start == b.start && a.title == b.title);
    all
}

fn ics_events() -> Vec<Ev> {
    let p = crate::config::data_dir().join("kalender.ics");
    let Ok(meta) = std::fs::metadata(&p) else { return vec![] };
    let mt = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    let mut g = ICS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((t, v)) = g.as_ref() {
        if *t == mt {
            return v.clone();
        }
    }
    let v = std::fs::read_to_string(&p).map(|t| parse_ics(&t)).unwrap_or_default();
    *g = Some((mt, v.clone()));
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn modes() {
        assert_eq!(classify("Code"), Mode::Coding);
        assert_eq!(classify("acad"), Mode::Teknik);
        assert_eq!(classify("WINWORD"), Mode::Dokumen);
        assert_eq!(classify("chrome"), Mode::Browsing);
        assert_eq!(Mode::Coding.agent(), 0); // satu orang saja: selalu NOVA
        assert_eq!(classify("explorer"), Mode::Lain);
    }
    #[test]
    fn ics_parsing() {
        let t = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nDTSTART:20261011T150000\nSUMMARY:Rapat\\, tim\n  proyek\nEND:VEVENT\nBEGIN:VEVENT\nDTSTART;VALUE=DATE:20261012\nSUMMARY:Libur\nEND:VEVENT\nEND:VCALENDAR";
        let e = parse_ics(t);
        assert_eq!(e.len(), 1);
        assert!(e[0].title.starts_with("Rapat, tim"));
        assert_eq!(e[0].start.format("%H:%M").to_string(), "15:00");
    }
}

/// Agen yang dipilih manual oleh Bos (4 = otomatis menurut konteks).
pub static OVERRIDE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
