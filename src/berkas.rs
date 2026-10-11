//! Perintah bahasa sehari-hari untuk komputer: "carikan file ...", "rapikan folder download", "matikan aplikasi yang tidak dipakai".
//! Keselamatan: pencarian hanya membaca nama/isi teks; merapikan hanya MEMINDAH (tidak pernah menghapus) dan bisa diurungkan;
//! menutup aplikasi memakai penutupan sopan (taskkill tanpa /F, aplikasi bisa menawarkan "simpan?"). Semua tindakan perubahan
//! menampilkan pratinjau lebih dulu dan baru dijalankan setelah Bos membalas "ya" di obrolan (model tidak bisa menerapkannya).
use crate::config::data_dir;
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

#[derive(Clone)]
enum Action {
    Rapikan { folder: PathBuf, moves: Vec<(PathBuf, PathBuf)> },
    TutupApp { groups: Vec<(String, Vec<u32>)> },
}

static PENDING: Mutex<Option<(Action, Instant)>> = Mutex::new(None);

fn set_pending(a: Action) {
    if let Ok(mut p) = PENDING.lock() {
        *p = Some((a, Instant::now()));
    }
}

pub fn has_pending() -> bool {
    PENDING.lock().map(|p| p.as_ref().is_some_and(|x| x.1.elapsed() < Duration::from_secs(300))).unwrap_or(false)
}

pub fn clear() {
    if let Ok(mut p) = PENDING.lock() {
        *p = None;
    }
}

fn no_window(c: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    #[cfg(not(windows))]
    let _ = c;
}

// ---------- Folder yang boleh disentuh ----------

fn roots() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = [dirs::document_dir(), dirs::download_dir(), dirs::desktop_dir(), dirs::picture_dir(), dirs::video_dir(), dirs::audio_dir()].into_iter().flatten().collect();
    v.extend(crate::proyek::load().projects.iter().map(PathBuf::from));
    v.dedup();
    v
}

fn forbidden(p: &Path) -> bool {
    let s = p.to_string_lossy().to_lowercase().replace('\\', "/");
    ["/windows", "/program files", "/programdata", "/appdata", "/$recycle.bin", "/system volume information"].iter().any(|k| s.contains(k)) || p.parent().is_none()
}

/// Terjemahkan sebutan folder ("download", "desktop", path) menjadi folder di dalam profil pengguna.
fn resolve_folder(arg: &str) -> Result<PathBuf, String> {
    let l = arg.to_lowercase();
    let named = if l.contains("download") || l.contains("unduh") || l.trim().is_empty() {
        dirs::download_dir()
    } else if l.contains("desktop") || l.contains("meja") {
        dirs::desktop_dir()
    } else if l.contains("dokumen") || l.contains("document") {
        dirs::document_dir()
    } else {
        None
    };
    if let Some(p) = named {
        return Ok(p);
    }
    let raw = arg.trim().trim_matches('"');
    let p = PathBuf::from(raw);
    let home = dirs::home_dir().ok_or("folder rumah tidak ditemukan")?;
    if p.is_dir() && p.starts_with(&home) && !forbidden(&p) {
        Ok(p)
    } else {
        Err("Sebutkan folder di dalam profil pengguna (mis. 'download', 'desktop', atau path di C:/Users/...).".into())
    }
}

// ---------- Cari file ----------

const STOP: &[&str] = &["carikan", "cari", "temukan", "tolong", "file", "berkas", "saya", "aku", "dong", "yang", "di", "mana", "ada", "dokumen", "milik", "ku", "nama", "bernama", "tentang", "dengan", "untuk", "itu", "ini"];

fn fmt_size(b: u64) -> String {
    if b >= 1_048_576 { format!("{:.1} MB", b as f64 / 1_048_576.0) } else { format!("{} KB", (b / 1024).max(1)) }
}

fn ext_hint(l: &str) -> Vec<&'static str> {
    let mut v = vec![];
    for (k, e) in [("pdf", &["pdf"][..]), ("word", &["doc", "docx"]), ("excel", &["xls", "xlsx", "csv"]), ("spreadsheet", &["xls", "xlsx", "csv"]), ("powerpoint", &["ppt", "pptx"]), ("presentasi", &["ppt", "pptx"]), ("gambar", &["jpg", "jpeg", "png", "gif", "webp"]), ("foto", &["jpg", "jpeg", "png", "heic"]), ("video", &["mp4", "mkv", "avi", "mov"]), ("musik", &["mp3", "wav", "flac", "m4a"]), ("lagu", &["mp3", "wav", "flac", "m4a"]), ("kode", &["py", "rs", "js", "ts", "go", "c", "cpp", "java"]), ("zip", &["zip", "rar", "7z"])] {
        if l.contains(k) {
            v.extend(e.iter());
        }
    }
    v
}

fn since_hint(l: &str) -> Option<Duration> {
    let d = |h: u64| Some(Duration::from_secs(h * 3600));
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

fn walk(dir: &Path, depth: u8, deadline: Instant, out: &mut Vec<(PathBuf, u64, SystemTime)>) {
    if depth > 6 || out.len() > 80_000 || Instant::now() > deadline {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let p = e.path();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() {
            if !name.starts_with('.') && !["node_modules", "target", "__pycache__", "$RECYCLE.BIN", "AppData"].contains(&name.as_str()) {
                walk(&p, depth + 1, deadline, out);
            }
        } else if let Ok(m) = e.metadata() {
            out.push((p, m.len(), m.modified().unwrap_or(SystemTime::UNIX_EPOCH)));
        }
    }
}

/// `cari <apa>`: nama (token), jenis (pdf/gambar/...), waktu (kemarin/minggu ini), dan isi ("berisi kata").
pub fn cari(query: &str) -> String {
    if let Some(r) = crate::indeks::cari(query) {
        return r;
    }
    // Indeks belum siap: cadangan penelusuran cepat folder pengguna.
    let l = query.to_lowercase();
    let (name_part, content) = match l.find("berisi ").or_else(|| l.find("mengandung ")) {
        Some(i) => (l[..i].to_string(), Some(l[i..].split_once(' ').map(|x| x.1.trim().to_string()).unwrap_or_default())),
        None => (l.clone(), None),
    };
    let exts = ext_hint(&l);
    let since = since_hint(&l);
    let tokens: Vec<String> = name_part
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 1 && !STOP.contains(w) && !["pdf", "word", "excel", "gambar", "foto", "video", "musik", "lagu", "kode", "zip", "kemarin", "hari", "ini", "minggu", "bulan", "powerpoint", "presentasi", "spreadsheet"].contains(w))
        .map(String::from)
        .collect();
    if tokens.is_empty() && exts.is_empty() && since.is_none() && content.is_none() {
        return "Mau cari file apa? Contoh: 'carikan file laporan pdf kemarin' atau 'cari file berisi invoice'.".into();
    }
    let deadline = Instant::now() + Duration::from_secs(6);
    let mut files = vec![];
    for r in roots() {
        walk(&r, 0, deadline, &mut files);
    }
    let now = SystemTime::now();
    let mut scored: Vec<(f32, PathBuf, u64, SystemTime)> = files
        .into_iter()
        .filter_map(|(p, size, mt)| {
            let name = p.file_name()?.to_string_lossy().to_lowercase();
            let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("").to_lowercase();
            if !exts.is_empty() && !exts.contains(&ext.as_str()) {
                return None;
            }
            if since.is_some_and(|s| now.duration_since(mt).map(|d| d > s).unwrap_or(false)) {
                return None;
            }
            let hit = tokens.iter().filter(|t| name.contains(t.as_str())).count();
            if !tokens.is_empty() && hit == 0 && content.is_none() {
                return None;
            }
            let frac = if tokens.is_empty() { 0.5 } else { hit as f32 / tokens.len() as f32 };
            let age_days = now.duration_since(mt).map(|d| d.as_secs_f32() / 86400.0).unwrap_or(999.0);
            Some((frac + 1.0 / (1.0 + age_days / 30.0) * 0.3, p, size, mt))
        })
        .collect();
    // Pencarian isi (hanya berkas teks kecil) bila diminta.
    if let Some(word) = content.filter(|w| !w.is_empty()) {
        let textual = ["txt", "md", "csv", "log", "json", "py", "rs", "js", "ts", "html", "css", "ini", "toml", "yaml", "yml", "sql"];
        let limit = Instant::now() + Duration::from_secs(8);
        scored.retain(|(_, p, size, _)| *size < 1_000_000 && p.extension().and_then(|x| x.to_str()).is_some_and(|e| textual.contains(&e.to_lowercase().as_str())));
        scored.retain(|(_, p, _, _)| Instant::now() < limit && std::fs::read_to_string(p).map(|t| t.to_lowercase().contains(&word)).unwrap_or(false));
    }
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    if scored.is_empty() {
        return "Tidak ketemu. Coba kata lain, atau sebut jenisnya (pdf/gambar/excel) dan waktunya (kemarin/minggu ini).".into();
    }
    let mut s = format!("🔎 Ketemu {} berkas (menampilkan 8 teratas):\n", scored.len());
    for (_, p, size, mt) in scored.iter().take(8) {
        let dt: DateTime<Local> = (*mt).into();
        s.push_str(&format!("📄 {} — {} ({}, {})\n", p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), p.parent().map(|d| d.display().to_string()).unwrap_or_default(), fmt_size(*size), dt.format("%d/%m %H:%M")));
    }
    s.push_str("Buka salah satu: ketik 'buka <path lengkap>'.");
    s
}

// ---------- Rapikan ----------

fn category(ext: &str) -> &'static str {
    match ext {
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "svg" | "heic" => "Gambar",
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "md" | "csv" | "odt" | "rtf" => "Dokumen",
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "webm" => "Video",
        "mp3" | "wav" | "flac" | "m4a" | "ogg" => "Musik",
        "zip" | "rar" | "7z" | "tar" | "gz" => "Arsip",
        "exe" | "msi" | "msix" | "iso" => "Installer",
        "py" | "js" | "ts" | "rs" | "go" | "c" | "cpp" | "h" | "java" | "json" | "html" | "css" | "sql" | "ipynb" => "Kode",
        _ => "Lainnya",
    }
}

/// Jumlah berkas lepas (tingkat atas) yang sudah lama di sebuah folder.
pub fn loose_count(folder: &Path) -> usize {
    let now = SystemTime::now();
    std::fs::read_dir(folder)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
                .filter(|e| e.metadata().and_then(|m| m.modified()).map(|t| now.duration_since(t).map(|d| d > Duration::from_secs(600)).unwrap_or(false)).unwrap_or(false))
                .count()
        })
        .unwrap_or(0)
}

/// Pratinjau: kelompokkan berkas lepas di folder (tanpa rekursi) ke subfolder menurut jenis.
pub fn rapikan(arg: &str) -> String {
    let folder = match resolve_folder(arg) {
        Ok(f) => f,
        Err(e) => return e,
    };
    let Ok(rd) = std::fs::read_dir(&folder) else { return format!("Tidak bisa membuka {}", folder.display()) };
    let now = SystemTime::now();
    let mut moves: Vec<(PathBuf, PathBuf)> = vec![];
    for e in rd.flatten() {
        let p = e.path();
        let Ok(ft) = e.file_type() else { continue };
        let name = e.file_name().to_string_lossy().to_string();
        let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("").to_lowercase();
        if !ft.is_file() || name.starts_with('.') || name.eq_ignore_ascii_case("desktop.ini") || ["tmp", "crdownload", "part", "lnk", "ini"].contains(&ext.as_str()) {
            continue;
        }
        // lewati yang baru diubah (mungkin sedang diunduh/dipakai)
        if e.metadata().and_then(|m| m.modified()).map(|t| now.duration_since(t).map(|d| d < Duration::from_secs(600)).unwrap_or(true)).unwrap_or(true) {
            continue;
        }
        let mut to = folder.join(category(&ext)).join(&name);
        let mut n = 1;
        while to.exists() {
            let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            to = folder.join(category(&ext)).join(format!("{stem} ({n}){}", if ext.is_empty() { String::new() } else { format!(".{ext}") }));
            n += 1;
        }
        moves.push((p, to));
    }
    if moves.is_empty() {
        return format!("{} sudah rapi (atau berisi berkas yang baru dipakai).", folder.display());
    }
    let mut by: BTreeMap<&str, usize> = BTreeMap::new();
    for (_, to) in &moves {
        *by.entry(to.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str()).unwrap_or("?")).or_default() += 1;
    }
    let mut s = format!("🧹 Rencana merapikan {} ({} berkas, tidak ada yang dihapus):\n", folder.display(), moves.len());
    for (k, n) in &by {
        s.push_str(&format!("• {k}: {n} berkas\n"));
    }
    for (from, to) in moves.iter().take(5) {
        s.push_str(&format!("   {} → {}/\n", from.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), to.parent().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_default()));
    }
    s.push_str("Ketik 'ya' untuk menjalankan (bisa diurungkan dengan 'urungkan'), atau 'tidak' untuk batal.");
    set_pending(Action::Rapikan { folder, moves });
    s
}

#[derive(Serialize, Deserialize)]
struct Moved {
    from: String,
    to: String,
}

pub fn undo_mtime() -> i64 {
    std::fs::metadata(undo_path()).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn undo_path() -> PathBuf {
    data_dir().join("urungkan.json")
}

fn apply_rapikan(moves: &[(PathBuf, PathBuf)]) -> String {
    let (mut ok, mut fail) = (vec![], 0);
    for (from, to) in moves {
        if from.exists() && !to.exists() {
            if let Some(d) = to.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            if std::fs::rename(from, to).is_ok() {
                ok.push(Moved { from: from.to_string_lossy().into(), to: to.to_string_lossy().into() });
                continue;
            }
        }
        fail += 1;
    }
    let _ = std::fs::write(undo_path(), serde_json::to_string(&ok).unwrap_or_default());
    format!("✅ {} berkas dirapikan{}. Salah rapikan? Ketik 'urungkan'.", ok.len(), if fail > 0 { format!(", {fail} dilewati") } else { String::new() })
}

pub fn urungkan() -> String {
    let Some(list) = std::fs::read_to_string(undo_path()).ok().and_then(|t| serde_json::from_str::<Vec<Moved>>(&t).ok()) else { return "Tidak ada rapikan terakhir yang bisa diurungkan.".into() };
    let mut n = 0;
    for m in list.iter().rev() {
        let (from, to) = (PathBuf::from(&m.from), PathBuf::from(&m.to));
        if to.exists() && !from.exists() && std::fs::rename(&to, &from).is_ok() {
            n += 1;
        }
    }
    let _ = std::fs::remove_file(undo_path());
    format!("↩ {n} berkas dikembalikan ke tempat semula.")
}

// ---------- Tutup aplikasi tidak terpakai ----------

const KEEP: &[&str] = &[
    "system", "system idle process", "registry", "smss.exe", "csrss.exe", "wininit.exe", "winlogon.exe", "services.exe", "lsass.exe", "svchost.exe", "explorer.exe", "dwm.exe", "fontdrvhost.exe", "sihost.exe", "ctfmon.exe", "taskhostw.exe",
    "runtimebroker.exe", "searchhost.exe", "startmenuexperiencehost.exe", "shellexperiencehost.exe", "textinputhost.exe", "conhost.exe", "dllhost.exe", "audiodg.exe", "spoolsv.exe", "wudfhost.exe", "securityhealthsystray.exe",
    "msmpeng.exe", "nissrv.exe", "nova.exe", "msedgewebview2.exe", "claude.exe", "ollama.exe",
];

fn used_recently() -> std::collections::HashSet<String> {
    let mut s = std::collections::HashSet::new();
    s.insert(crate::konteks::current_app().to_lowercase());
    if let Ok(c) = crate::assistant::db() {
        let h = chrono::Timelike::hour(&Local::now()) as i64;
        let day = Local::now().format("%Y-%m-%d").to_string();
        if let Ok(mut st) = c.prepare("SELECT DISTINCT app FROM usage WHERE day=?1 AND hour IN (?2, ?3)") {
            if let Ok(rows) = st.query_map((&day, h, (h + 23) % 24), |r| r.get::<_, String>(0)) {
                s.extend(rows.flatten().map(|a| a.to_lowercase()));
            }
        }
    }
    s
}

/// Pratinjau aplikasi berat yang tidak dipakai dalam ~1 jam terakhir.
pub fn apps() -> String {
    let used = used_recently();
    let mut sys = sysinfo::System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    let mut groups: HashMap<String, (Vec<u32>, u64)> = HashMap::new();
    for (pid, p) in sys.processes() {
        let name = p.name().to_string_lossy().to_lowercase();
        if KEEP.contains(&name.as_str()) || pid.as_u32() <= 4 || pid.as_u32() == std::process::id() {
            continue;
        }
        let key = name.trim_end_matches(".exe").to_string();
        if used.contains(&key) {
            continue;
        }
        let g = groups.entry(name).or_default();
        g.0.push(pid.as_u32());
        g.1 += p.memory();
    }
    let mut list: Vec<(String, Vec<u32>, u64)> = groups.into_iter().filter(|(_, g)| g.1 >= 120 * 1_048_576).map(|(n, g)| (n, g.0, g.1)).collect();
    list.sort_by(|a, b| b.2.cmp(&a.2));
    list.truncate(8);
    if list.is_empty() {
        return "Tidak ada aplikasi berat yang menganggur. Semua sedang terpakai atau ringan 👍".into();
    }
    let total: u64 = list.iter().map(|x| x.2).sum();
    let mut s = format!("🧹 Aplikasi yang tidak kamu pakai dalam ±1 jam (total ±{:.1} GB RAM):\n", total as f64 / 1_073_741_824.0);
    for (n, pids, mem) in &list {
        s.push_str(&format!("• {n} ×{} ({} MB)\n", pids.len(), mem / 1_048_576));
    }
    s.push_str("Ketik 'ya' untuk menutupnya dengan sopan (aplikasi bisa menanyakan 'simpan?'), atau 'tidak' untuk batal.");
    set_pending(Action::TutupApp { groups: list.into_iter().map(|(n, p, _)| (n, p)).collect() });
    s
}

fn apply_tutup(groups: &[(String, Vec<u32>)]) -> String {
    let (mut closed, mut stuck) = (vec![], vec![]);
    for (name, pids) in groups {
        let mut any = false;
        for pid in pids {
            let mut c = std::process::Command::new("taskkill");
            c.args(["/PID", &pid.to_string()]); // tanpa /F: penutupan sopan
            no_window(&mut c);
            if c.output().map(|o| o.status.success()).unwrap_or(false) {
                any = true;
            }
        }
        if any { closed.push(name.clone()) } else { stuck.push(name.clone()) }
    }
    format!("✅ Diminta menutup: {}.{}", if closed.is_empty() { "(tidak ada)".into() } else { closed.join(", ") }, if stuck.is_empty() { String::new() } else { format!("\nℹ Tidak mau ditutup dengan sopan (proses latar tanpa jendela): {}. Kubiarkan agar tidak ada data hilang.", stuck.join(", ")) })
}

/// Balasan 'ya' dari Bos: jalankan tindakan yang menunggu. None bila tidak ada.
pub fn confirm() -> Option<String> {
    let (a, at) = PENDING.lock().ok()?.take()?;
    if at.elapsed() > Duration::from_secs(300) {
        return Some("Permintaan tadi sudah kedaluwarsa. Ulangi perintahnya ya.".into());
    }
    Some(match a {
        Action::Rapikan { moves, .. } => apply_rapikan(&moves),
        Action::TutupApp { groups } => apply_tutup(&groups),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn categories_and_folder_rules() {
        assert_eq!(category("pdf"), "Dokumen");
        assert_eq!(category("png"), "Gambar");
        assert_eq!(category("xyz"), "Lainnya");
        assert!(forbidden(Path::new("C:/Windows/System32")));
        assert!(!forbidden(Path::new("C:/Users/me/Downloads")));
        assert!(resolve_folder("C:/Windows").is_err());
    }
    #[test]
    fn rapikan_moves_old_files_and_undo() {
        let d = dirs::home_dir().unwrap().join("nova-rapikan-test");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("laporan.pdf");
        std::fs::write(&f, "x").unwrap();
        // jadikan lama (>10 menit) agar tidak dilewati
        let old = SystemTime::now() - Duration::from_secs(3600);
        let h = std::fs::File::options().write(true).open(&f).unwrap();
        h.set_modified(old).unwrap();
        drop(h);
        let r = rapikan(&d.to_string_lossy());
        assert!(r.contains("Dokumen: 1"), "{r}");
        assert!(confirm().unwrap().contains("1 berkas dirapikan"));
        assert!(d.join("Dokumen").join("laporan.pdf").exists());
        assert!(urungkan().contains("1 berkas dikembalikan"));
        assert!(f.exists());
        let _ = std::fs::remove_dir_all(&d);
    }
}
