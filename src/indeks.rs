//! Indeks berkas seluruh drive tetap (C: folder pengguna + semua drive lain) di SQLite, dibangun di latar dengan prioritas rendah.
//! Pencarian "carikan file ..." jadi instan dan menjangkau semua drive; hasil teratas langsung dibuka di Explorer.
//! Dilewati: folder sistem/cache (Windows, Program Files, AppData, node_modules, .git, build cache). Hanya nama + path + ukuran + tanggal
//! yang disimpan (bukan isi berkas); isi hanya dibaca saat Bos meminta "berisi kata ...".
use crate::config::data_dir;
use chrono::{DateTime, Local};
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

static BUILDING: AtomicBool = AtomicBool::new(false);
static PROGRESS: AtomicU64 = AtomicU64::new(0);
static LAST: Mutex<Vec<Hit>> = Mutex::new(Vec::new());

#[derive(Clone, Debug)]
pub struct Hit {
    pub path: String,
    pub size: u64,
    pub mt: i64,
}

fn db() -> rusqlite::Result<Connection> {
    let c = Connection::open(data_dir().join("indeks.db"))?;
    c.busy_timeout(Duration::from_secs(10))?;
    let _ = c.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;");
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS f(path TEXT PRIMARY KEY, nl TEXT, ext TEXT, size INT, mt INT, gen INT);
         CREATE INDEX IF NOT EXISTS f_nl ON f(nl);
         CREATE TABLE IF NOT EXISTS meta(k TEXT PRIMARY KEY, v TEXT);",
    )?;
    Ok(c)
}

#[cfg(windows)]
fn fixed_drives() -> Vec<PathBuf> {
    use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};
    let mask = unsafe { GetLogicalDrives() };
    (0..26)
        .filter(|i| mask & (1 << i) != 0)
        .filter_map(|i| {
            let root = format!("{}:\\", (b'A' + i as u8) as char);
            let w: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
            (unsafe { GetDriveTypeW(w.as_ptr()) } == 3).then(|| PathBuf::from(root)) // DRIVE_FIXED
        })
        .collect()
}

#[cfg(not(windows))]
fn fixed_drives() -> Vec<PathBuf> {
    dirs::home_dir().into_iter().collect()
}

/// Folder yang diindeks: drive sistem -> folder pengguna + menu Start; drive lain -> seluruhnya.
pub fn roots() -> Vec<PathBuf> {
    let sys = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into()).to_uppercase();
    let mut v = vec![];
    for d in fixed_drives() {
        if d.to_string_lossy().to_uppercase().starts_with(&sys) {
            if let Some(h) = dirs::home_dir() {
                v.push(h);
            }
            if let Ok(pd) = std::env::var("ProgramData") {
                v.push(PathBuf::from(pd).join("Microsoft").join("Windows").join("Start Menu"));
            }
            if let Some(a) = dirs::data_dir() {
                v.push(a.join("Microsoft").join("Windows").join("Start Menu"));
            }
        } else {
            v.push(d);
        }
    }
    v
}

const SKIP_DIRS: &[&str] = &[
    ".git", "node_modules", "__pycache__", ".venv", "venv", ".cache", "incremental", ".fingerprint", "$recycle.bin", "system volume information", "appdata", "windows",
    "program files", "program files (x86)", "programdata", "msocache", "recovery", "perflogs", ".cargo", ".rustup", ".npm", ".gradle", ".m2", ".nuget", "steamlibrary", "ollamamodels", ".trash-1000", "deps",
];
const SKIP_EXT: &[&str] = &["rlib", "rmeta", "d", "o", "obj", "pdb", "ilk", "exp", "tmp", "pyc", "class", "lock", "cache", "dll", "sys", "etl", "pack", "idx"];

/// Bangun ulang indeks (blocking, ramah CPU). Mengembalikan jumlah berkas.
pub fn rebuild() -> u64 {
    if BUILDING.swap(true, Ordering::SeqCst) {
        return 0;
    }
    PROGRESS.store(0, Ordering::Relaxed);
    let started = Instant::now();
    let generation = Local::now().timestamp();
    let result = (|| -> rusqlite::Result<u64> {
        let mut c = db()?;
        let mut total = 0u64;
        let mut tx = c.transaction()?;
        let mut pending = 0;
        'outer: for root in roots() {
            let mut stack = vec![root];
            while let Some(dir) = stack.pop() {
                let Ok(rd) = std::fs::read_dir(&dir) else { continue };
                for e in rd.flatten() {
                    let name = e.file_name().to_string_lossy().to_string();
                    let Ok(ft) = e.file_type() else { continue };
                    if ft.is_dir() {
                        if !SKIP_DIRS.contains(&name.to_lowercase().as_str()) {
                            stack.push(e.path());
                        }
                        continue;
                    }
                    let ext = Path::new(&name).extension().and_then(|x| x.to_str()).unwrap_or("").to_lowercase();
                    if SKIP_EXT.contains(&ext.as_str()) {
                        continue;
                    }
                    let (size, mt) = e.metadata().map(|m| (m.len(), m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0))).unwrap_or((0, 0));
                    tx.execute("INSERT OR REPLACE INTO f VALUES(?1,?2,?3,?4,?5,?6)", (e.path().to_string_lossy().as_ref(), name.to_lowercase(), &ext, size as i64, mt, generation))?;
                    total += 1;
                    pending += 1;
                    PROGRESS.store(total, Ordering::Relaxed);
                    if pending >= 4000 {
                        tx.commit()?;
                        tx = c.transaction()?;
                        pending = 0;
                        std::thread::sleep(Duration::from_millis(40)); // beri napas pada CPU/disk
                    }
                    if total >= 700_000 || started.elapsed() > Duration::from_secs(25 * 60) {
                        break 'outer;
                    }
                }
            }
        }
        tx.commit()?;
        c.execute("DELETE FROM f WHERE gen < ?1", [generation])?;
        c.execute("INSERT OR REPLACE INTO meta VALUES('last_scan', ?1)", [generation.to_string()])?;
        Ok(total)
    })();
    BUILDING.store(false, Ordering::SeqCst);
    result.unwrap_or(0)
}

pub fn count() -> i64 {
    db().ok().and_then(|c| c.query_row("SELECT COUNT(*) FROM f", [], |r| r.get(0)).ok()).unwrap_or(0)
}

fn age_hours() -> i64 {
    let last: i64 = db().ok().and_then(|c| c.query_row("SELECT v FROM meta WHERE k='last_scan'", [], |r| r.get::<_, String>(0)).ok()).and_then(|s| s.parse().ok()).unwrap_or(0);
    (Local::now().timestamp() - last) / 3600
}

/// Mulai pembangunan di thread terpisah bila indeks kosong/usang (>8 jam).
pub fn ensure_fresh() {
    if !BUILDING.load(Ordering::Relaxed) && (count() < 1000 || age_hours() > 8) {
        std::thread::spawn(|| {
            rebuild();
        });
    }
}

pub fn status() -> String {
    format!(
        "🗂 Indeks berkas: {} berkas{} · drive: {}",
        count(),
        if BUILDING.load(Ordering::Relaxed) { format!(" (sedang dibangun, {} dipindai)", PROGRESS.load(Ordering::Relaxed)) } else { String::new() },
        roots().iter().map(|r| r.display().to_string()).collect::<Vec<_>>().join(", ")
    )
}

// ---------- Pencarian ----------

const STOP: &[&str] = &[
    "halo", "hai", "coba", "cari", "carikan", "cariin", "temukan", "tolong", "bantu", "file", "berkas", "dokumen", "program", "aplikasi", "app", "saya", "aku", "dong", "yang", "di", "mana", "ada", "milik", "bernama", "nama", "folder",
    "laptop", "komputer", "pc", "tentang", "dengan", "untuk", "itu", "ini", "buka", "tunjukkan", "lihat", "kemarin", "hari", "minggu", "bulan", "dan", "atau", "ke", "dari", "semua", "punya", "mau", "ingin", "dalam", "sebuah",
];

fn ext_hint(l: &str) -> Vec<&'static str> {
    let mut v = vec![];
    for (k, e) in [
        ("pdf", &["pdf"][..]), ("word", &["doc", "docx"]), ("excel", &["xls", "xlsx", "csv"]), ("spreadsheet", &["xls", "xlsx", "csv"]), ("powerpoint", &["ppt", "pptx"]), ("presentasi", &["ppt", "pptx"]),
        ("gambar", &["jpg", "jpeg", "png", "gif", "webp"]), ("foto", &["jpg", "jpeg", "png", "heic"]), ("video", &["mp4", "mkv", "avi", "mov"]), ("musik", &["mp3", "wav", "flac", "m4a"]), ("lagu", &["mp3", "wav", "flac", "m4a"]),
        ("kode", &["py", "rs", "js", "ts", "go", "c", "cpp", "java"]), ("zip", &["zip", "rar", "7z"]),
    ] {
        if l.contains(k) {
            v.extend(e.iter());
        }
    }
    if l.split_whitespace().any(|w| ["program", "aplikasi", "app", "exe"].contains(&w)) {
        v.extend(["exe", "lnk", "msi"]);
    }
    v
}

fn since_hint(l: &str) -> Option<i64> {
    let d = |h: i64| Some(Local::now().timestamp() - h * 3600);
    if l.contains("hari ini") {
        d(24)
    } else if l.contains("kemarin") {
        d(48)
    } else if l.contains("minggu ini") || l.contains("seminggu") {
        d(24 * 7)
    } else if l.contains("bulan ini") || l.contains("sebulan") {
        d(24 * 31)
    } else {
        None
    }
}

/// Token pencarian dari kalimat bebas.
pub fn tokens(q: &str) -> Vec<String> {
    let skip_words = ["pdf", "word", "excel", "gambar", "foto", "video", "musik", "lagu", "kode", "zip", "powerpoint", "presentasi", "spreadsheet", "exe"];
    q.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 2 && !STOP.contains(w) && !skip_words.contains(w))
        .map(String::from)
        .collect()
}

fn like(s: &str) -> String {
    format!("%{}%", s.replace('%', "").replace('_', "\\_"))
}

fn fmt_size(b: u64) -> String {
    if b >= 1_048_576 { format!("{:.1} MB", b as f64 / 1_048_576.0) } else { format!("{} KB", (b / 1024).max(1)) }
}

/// Cari di indeks. None bila indeks belum siap (pemanggil memakai cadangan).
pub fn cari(query: &str) -> Option<String> {
    let n = count();
    if n < 1000 {
        return None;
    }
    let l = query.to_lowercase();
    let (name_part, content) = match l.find("berisi ").or_else(|| l.find("mengandung ")) {
        Some(i) => (l[..i].to_string(), Some(l[i..].split_once(' ').map(|x| x.1.trim().to_string()).unwrap_or_default())),
        None => (l.clone(), None),
    };
    let toks = tokens(&name_part);
    let exts = ext_hint(&name_part);
    let since = since_hint(&name_part);
    if toks.is_empty() && exts.is_empty() && since.is_none() && content.is_none() {
        return Some("Mau cari apa? Contoh: 'carikan file motor listrik pdf', 'cari program nova', 'cari file berisi invoice kemarin'.".into());
    }
    let c = db().ok()?;
    // 1) semua kata ada di NAMA; 2) bila sedikit: semua kata ada di PATH (folder); 3) bila tetap kosong: salah satu kata.
    let run = |mode: u8| -> Vec<Hit> {
        let mut sql = String::from("SELECT path, size, mt FROM f WHERE 1=1");
        let mut params: Vec<String> = vec![];
        for t in &toks {
            match mode {
                0 => sql.push_str(" AND nl LIKE ? ESCAPE '\\'"),
                _ => sql.push_str(" AND lower(path) LIKE ? ESCAPE '\\'"),
            }
            params.push(like(t));
        }
        if mode == 2 && !toks.is_empty() {
            sql = String::from("SELECT path, size, mt FROM f WHERE (");
            sql.push_str(&toks.iter().map(|_| "nl LIKE ? ESCAPE '\\'").collect::<Vec<_>>().join(" OR "));
            sql.push(')');
            params = toks.iter().map(|t| like(t)).collect();
        }
        if !exts.is_empty() {
            sql.push_str(&format!(" AND ext IN ({})", exts.iter().map(|e| format!("'{e}'")).collect::<Vec<_>>().join(",")));
        }
        if let Some(s) = since {
            sql.push_str(&format!(" AND mt >= {s}"));
        }
        sql.push_str(" ORDER BY mt DESC LIMIT 400");
        c.prepare(&sql)
            .and_then(|mut st| st.query_map(rusqlite::params_from_iter(params.iter()), |r| Ok(Hit { path: r.get(0)?, size: r.get::<_, i64>(1)? as u64, mt: r.get(2)? })).map(|r| r.flatten().collect()))
            .unwrap_or_default()
    };
    let mut hits = run(0);
    let mut where_ = "nama file";
    if hits.len() < 3 && !toks.is_empty() {
        let more = run(1);
        if !more.is_empty() {
            hits.extend(more);
            where_ = "nama atau folder";
        }
    }
    if hits.is_empty() && toks.len() > 1 {
        hits = run(2);
        where_ = "salah satu kata";
    }
    // Pencarian isi berkas teks (opsional)
    if let Some(word) = content.filter(|w| !w.is_empty()) {
        let textual = ["txt", "md", "csv", "log", "json", "py", "rs", "js", "ts", "html", "css", "ini", "toml", "yaml", "yml", "sql"];
        let limit = Instant::now() + Duration::from_secs(8);
        if hits.is_empty() {
            hits = c
                .prepare("SELECT path, size, mt FROM f WHERE ext IN ('txt','md','csv','log','json','py','rs','js','ts','html','ini','toml','yaml','sql') AND size < 1000000 ORDER BY mt DESC LIMIT 2000")
                .and_then(|mut st| st.query_map([], |r| Ok(Hit { path: r.get(0)?, size: r.get::<_, i64>(1)? as u64, mt: r.get(2)? })).map(|r| r.flatten().collect()))
                .unwrap_or_default();
        }
        hits.retain(|h| {
            h.size < 1_000_000 && Path::new(&h.path).extension().and_then(|x| x.to_str()).is_some_and(|e| textual.contains(&e.to_lowercase().as_str())) && Instant::now() < limit && std::fs::read_to_string(&h.path).map(|t| t.to_lowercase().contains(&word)).unwrap_or(false)
        });
        where_ = "isi berkas";
    }
    // Skor: kecocokan nama, awalan, kedekatan ke akar, kebaruan
    let now = Local::now().timestamp();
    let mut scored: Vec<(f32, Hit)> = hits
        .into_iter()
        .map(|h| {
            let name = Path::new(&h.path).file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
            let m = toks.iter().filter(|t| name.contains(t.as_str())).count() as f32;
            let starts = toks.iter().any(|t| name.starts_with(t.as_str())) as u8 as f32;
            let depth = h.path.matches(['\\', '/']).count() as f32;
            let age_days = ((now - h.mt).max(0) as f32) / 86400.0;
            (m * 2.0 + starts + 1.0 / (1.0 + age_days / 60.0) - depth * 0.03, h)
        })
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    scored.dedup_by(|a, b| a.1.path == b.1.path);
    let top: Vec<Hit> = scored.into_iter().map(|x| x.1).take(8).collect();
    if top.is_empty() {
        return Some(format!(
            "Tidak ketemu di {n} berkas terindeks (semua drive tetap). Kata yang kucari: {}{}. Coba kata lain/ejaan lain, atau sebut jenis & waktunya. {}",
            if toks.is_empty() { "(hanya filter)".into() } else { toks.join(", ") },
            if exts.is_empty() { String::new() } else { format!(" · jenis: {}", exts.join("/")) },
            if BUILDING.load(Ordering::Relaxed) { "Catatan: indeks masih dibangun, hasil bisa belum lengkap." } else { "Ketik /indeks untuk melihat cakupan indeks." }
        ));
    }
    let mut s = format!("🔎 Ketemu di {where_} (menampilkan {} teratas dari indeks {n} berkas):\n", top.len());
    for (i, h) in top.iter().enumerate() {
        let p = Path::new(&h.path);
        let dt = DateTime::from_timestamp(h.mt, 0).map(|t| t.with_timezone(&Local).format("%d/%m/%y").to_string()).unwrap_or_default();
        s.push_str(&format!("{}. {} — {} ({}, {})\n", i + 1, p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), p.parent().map(|d| d.display().to_string()).unwrap_or_default(), fmt_size(h.size), dt));
    }
    s.push_str("Folder hasil #1 kubuka di Explorer. Untuk yang lain: 'buka folder 2'.");
    if let Ok(mut l) = LAST.lock() {
        *l = top;
    }
    Some(s)
}

/// Buka Explorer dengan berkas hasil ke-n terpilih (1-based). Mengembalikan teks konfirmasi.
pub fn reveal(n: usize) -> String {
    let Some(h) = LAST.lock().ok().and_then(|l| l.get(n.saturating_sub(1)).cloned()) else { return "Belum ada hasil pencarian untuk dibuka. Cari dulu: 'carikan file ...'.".into() };
    let mut c = std::process::Command::new("explorer");
    c.arg(format!("/select,{}", h.path.replace('/', "\\")));
    match c.spawn() {
        Ok(_) => format!("📂 Membuka folder: {}", Path::new(&h.path).parent().map(|d| d.display().to_string()).unwrap_or_default()),
        Err(e) => format!("Gagal membuka Explorer: {e}"),
    }
}

/// Ekstensi yang aman dibuka langsung dengan aplikasi bawaannya (dokumen, gambar, media, desain). Yang bisa dieksekusi
/// (exe, bat, ps1, msi, ...) tidak pernah dibuka otomatis.
const SAFE_OPEN: &[&str] = &["pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt", "md", "csv", "rtf", "odt", "png", "jpg", "jpeg", "gif", "bmp", "webp", "svg", "drawio", "dwg", "dxf", "mp3", "wav", "flac", "m4a", "mp4", "mkv", "avi", "mov", "json", "xml", "log", "html", "ini", "yaml", "yml", "toml", "rs", "py", "js", "ts", "c", "cpp", "h", "cs", "java", "sql", "ino", "plc"];

/// Buka berkas hasil ke-n LANGSUNG dengan aplikasi bawaannya (hanya ekstensi aman); selain itu tunjukkan foldernya.
pub fn open_file(n: usize) -> String {
    let Some(h) = LAST.lock().ok().and_then(|l| l.get(n.saturating_sub(1)).cloned()) else { return "Belum ada hasil pencarian untuk dibuka.".into() };
    open_exact(&h.path)
}

/// Buka satu jalur berkas persis (ekstensi aman -> aplikasi bawaan; selain itu tunjukkan di Explorer).
pub fn open_exact(path: &str) -> String {
    let h = Hit { path: path.to_string(), size: 0, mt: 0 };
    let ext = Path::new(&h.path).extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    let name = Path::new(&h.path).file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
    if !SAFE_OPEN.contains(&ext.as_str()) {
        let mut c = std::process::Command::new("explorer");
        c.arg(format!("/select,{}", h.path.replace('/', "\\")));
        let _ = c.spawn();
        return format!("'{name}' bertipe .{ext} (bisa dieksekusi/tidak dikenal) sehingga tidak kubuka otomatis; kutunjukkan di folder.");
    }
    let mut c = std::process::Command::new("cmd");
    c.args(["/C", "start", "", &h.path.replace('/', "\\")]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    match c.spawn() {
        Ok(_) => format!("📄 Membuka {name}"),
        Err(e) => format!("Gagal membuka {name}: {e}"),
    }
}

pub fn last_count() -> usize {
    LAST.lock().map(|l| l.len()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tokenization_drops_filler() {
        assert_eq!(tokens("coba cari file program nova"), vec!["nova"]);
        assert_eq!(tokens("coba cari file pdf electric motor"), vec!["electric", "motor"]);
        assert!(ext_hint("coba cari file program nova").contains(&"exe"));
        assert!(ext_hint("cari file pdf electric motor").contains(&"pdf"));
        assert!(since_hint("file kemarin").is_some());
    }
    #[test]
    fn roots_include_other_fixed_drives_or_home() {
        assert!(!roots().is_empty());
    }
}
