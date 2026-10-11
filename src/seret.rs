//! Seret & lepas / pilih berkas atau folder: NOVA menampilkan INFORMASInya (jenis, ukuran, tanggal, isi folder, dll.) lalu
//! menawarkan tindakan lewat tombol (Buka, Ringkas, Folder, Petakan, Rapikan). Hanya metadata yang dibaca; isi berkas baru
//! dibaca bila Bos menekan Ringkas/Pelajari. Konteks seret berlaku 15 menit dan membatasi alat `bukapath` ke jalur itu saja.
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

struct Ctx {
    path: PathBuf,
    is_dir: bool,
    at: Instant,
}

static CTX: Mutex<Option<Ctx>> = Mutex::new(None);

fn human(b: u64) -> String {
    let (mut v, mut i) = (b as f64, 0);
    while v >= 1024.0 && i < 4 {
        v /= 1024.0;
        i += 1;
    }
    format!("{:.1} {}", v, ["B", "KB", "MB", "GB", "TB"][i]).replace(".0 B", " B")
}

fn when(t: std::io::Result<SystemTime>) -> String {
    t.ok().map(|t| chrono::DateTime::<chrono::Local>::from(t).format("%d/%m/%Y %H:%M").to_string()).unwrap_or_else(|| "-".into())
}

fn kind_of(ext: &str) -> &'static str {
    match ext {
        "pdf" => "Dokumen PDF",
        "doc" | "docx" => "Dokumen Word",
        "xls" | "xlsx" | "csv" => "Lembar kerja / tabel",
        "ppt" | "pptx" => "Presentasi",
        "txt" | "md" | "log" => "Teks",
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "svg" => "Gambar",
        "mp4" | "mkv" | "avi" | "mov" => "Video",
        "mp3" | "wav" | "flac" | "m4a" => "Audio",
        "zip" | "rar" | "7z" | "tar" | "gz" => "Arsip",
        "exe" | "msi" | "bat" | "ps1" | "cmd" => "Program/skrip (dapat dieksekusi)",
        "rs" | "py" | "js" | "ts" | "c" | "cpp" | "h" | "cs" | "java" | "go" | "ino" | "sql" => "Kode sumber",
        "dwg" | "dxf" | "drawio" => "Gambar teknik / diagram",
        "json" | "xml" | "yaml" | "yml" | "toml" | "ini" => "Konfigurasi / data",
        "" => "Tanpa ekstensi",
        _ => "Berkas",
    }
}

fn clean(p: &str) -> String {
    p.trim().trim_matches('"').trim_matches('\'').to_string()
}

/// Dipanggil saat berkas/folder dilepas atau dipilih. Mengembalikan (teks info, opsi tombol).
pub fn info(raw: &str) -> (String, Vec<String>) {
    let p = PathBuf::from(clean(raw));
    let Ok(md) = std::fs::metadata(&p) else { return (format!("Tidak bisa membaca '{}': tidak ada atau akses ditolak.", p.display()), vec![]) };
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
    if let Ok(mut c) = CTX.lock() {
        *c = Some(Ctx { path: p.clone(), is_dir: md.is_dir(), at: Instant::now() });
    }
    if md.is_dir() {
        let (files, dirs, total, exts, big, newest) = scan_dir(&p);
        let mut t = format!("📁 {name}\nLokasi: {}\nIsi: {files} berkas, {dirs} subfolder · total {}\nDiubah: {}\n", p.parent().map(|d| d.display().to_string()).unwrap_or_default(), human(total), when(md.modified()));
        if !exts.is_empty() {
            t.push_str(&format!("Jenis terbanyak: {}\n", exts.iter().map(|(e, n)| format!(".{e} ×{n}")).collect::<Vec<_>>().join(", ")));
        }
        if !big.is_empty() {
            t.push_str(&format!("Terbesar: {}\n", big.iter().map(|(n, s)| format!("{n} ({})", human(*s))).collect::<Vec<_>>().join(", ")));
        }
        if let Some((n, d)) = newest {
            t.push_str(&format!("Terbaru: {n} ({d})\n"));
        }
        return (t.trim_end().to_string(), vec!["Buka".into(), "Petakan".into(), "Rapikan".into()]);
    }
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    let mut t = format!("📄 {name}\nJenis: {} (.{ext})\nUkuran: {}\nDiubah: {} · Dibuat: {}\nLokasi: {}", kind_of(&ext), human(md.len()), when(md.modified()), when(md.created()), p.parent().map(|d| d.display().to_string()).unwrap_or_default());
    if ["txt", "md", "csv", "log", "rs", "py", "js", "ts", "json", "xml", "yaml", "yml", "toml", "ini", "c", "cpp", "h", "cs", "java", "sql", "ino"].contains(&ext.as_str()) && md.len() < 3_000_000 {
        if let Ok(s) = std::fs::read_to_string(&p) {
            t.push_str(&format!("\nIsi: {} baris, {} kata", s.lines().count(), s.split_whitespace().count()));
        }
    }
    if ext == "pdf" && md.len() < 40_000_000 {
        if let Ok(b) = std::fs::read(&p) {
            let s = String::from_utf8_lossy(&b);
            let n = (s.matches("/Type /Page").count() + s.matches("/Type/Page").count()).saturating_sub(s.matches("/Type /Pages").count() + s.matches("/Type/Pages").count());
            if n > 0 {
                t.push_str(&format!("\nHalaman: ±{n}"));
            }
        }
    }
    let mut opts = vec!["Buka".to_string()];
    if ["pdf", "txt", "md"].contains(&ext.as_str()) {
        opts.push("Ringkas".into());
    }
    opts.push("Folder".into());
    (t, opts)
}

fn scan_dir(p: &Path) -> (usize, usize, u64, Vec<(String, usize)>, Vec<(String, u64)>, Option<(String, String)>) {
    let start = Instant::now();
    let (mut files, mut dirs, mut total, mut seen) = (0usize, 0usize, 0u64, 0usize);
    let mut ext: std::collections::HashMap<String, usize> = Default::default();
    let mut big: Vec<(String, u64)> = vec![];
    let mut newest: Option<(SystemTime, String)> = None;
    let mut stack = vec![p.to_path_buf()];
    'outer: while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            seen += 1;
            if seen > 8000 || start.elapsed() > Duration::from_secs(2) {
                break 'outer;
            }
            let Ok(md) = e.metadata() else { continue };
            let name = e.file_name().to_string_lossy().to_string();
            if md.is_dir() {
                if !["node_modules", ".git", "target", "$RECYCLE.BIN"].contains(&name.as_str()) {
                    dirs += 1;
                    stack.push(e.path());
                }
            } else {
                files += 1;
                total += md.len();
                *ext.entry(Path::new(&name).extension().and_then(|x| x.to_str()).unwrap_or("-").to_lowercase()).or_default() += 1;
                big.push((name.clone(), md.len()));
                if let Ok(m) = md.modified() {
                    if newest.as_ref().map_or(true, |(t, _)| m > *t) {
                        newest = Some((m, name));
                    }
                }
            }
        }
    }
    let mut ex: Vec<(String, usize)> = ext.into_iter().collect();
    ex.sort_by_key(|x| std::cmp::Reverse(x.1));
    ex.truncate(4);
    big.sort_by_key(|x| std::cmp::Reverse(x.1));
    big.truncate(3);
    (files, dirs, total, ex, big, newest.map(|(t, n)| (n, when(Ok(t)))))
}

/// Jalur yang boleh dibuka oleh alat `bukapath`: konteks seret aktif (<15 menit), jalur itu atau induk/isinya.
pub fn is_dropped(path: &str) -> bool {
    let p = PathBuf::from(clean(path));
    CTX.lock().ok().and_then(|c| c.as_ref().map(|c| c.at.elapsed() < Duration::from_secs(900) && (p == c.path || c.path.parent().is_some_and(|d| p == d) || (c.is_dir && p.starts_with(&c.path))))).unwrap_or(false)
}

/// Label tombol -> perintah obrolan. None bila bukan jawaban untuk berkas yang sedang aktif.
pub fn resolve(t: &str) -> Option<String> {
    let c = CTX.lock().ok()?;
    let c = c.as_ref()?;
    if c.at.elapsed() > Duration::from_secs(900) {
        return None;
    }
    let (path, parent) = (c.path.display().to_string(), c.path.parent().map(|d| d.display().to_string()).unwrap_or_default());
    match (t.trim().to_lowercase().as_str(), c.is_dir) {
        ("buka", _) => Some(format!("bukapath {path}")),
        ("folder", false) => Some(format!("bukapath {parent}")),
        ("ringkas", false) => Some(format!("ringkas {path}")),
        ("petakan", true) => Some(format!("graphify {path}")),
        ("rapikan", true) => Some(format!("rapikan {path}")),
        _ => None,
    }
}

/// Dialog pilih berkas/folder Windows (dijalankan di proses NOVA; blokir sampai dipilih/dibatalkan).
pub fn pick(folder: bool) -> Option<String> {
    let script = if folder {
        "Add-Type -AssemblyName System.Windows.Forms; $f=New-Object System.Windows.Forms.Form; $f.TopMost=$true; $f.ShowInTaskbar=$false; $f.WindowState='Minimized'; $f.Show(); $d=New-Object System.Windows.Forms.FolderBrowserDialog; $d.Description='Pilih folder untuk NOVA'; if($d.ShowDialog($f) -eq 'OK'){ $d.SelectedPath }"
    } else {
        "Add-Type -AssemblyName System.Windows.Forms; $f=New-Object System.Windows.Forms.Form; $f.TopMost=$true; $f.ShowInTaskbar=$false; $f.WindowState='Minimized'; $f.Show(); $d=New-Object System.Windows.Forms.OpenFileDialog; $d.Title='Pilih berkas untuk NOVA'; if($d.ShowDialog($f) -eq 'OK'){ $d.FileName }"
    };
    let mut c = std::process::Command::new("powershell");
    c.args(["-NoProfile", "-STA", "-ExecutionPolicy", "Bypass", "-Command", script]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let o = c.output().ok()?;
    let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

/// Buka jalur (berkas aman dengan aplikasi bawaan, folder di Explorer, selain itu tunjukkan di folder).
pub fn open_path(path: &str) -> String {
    let p = PathBuf::from(clean(path));
    if p.is_dir() {
        let _ = std::process::Command::new("explorer").arg(&p).spawn();
        return format!("📂 Membuka folder {}", p.display());
    }
    crate::indeks::open_exact(&p.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn info_for_folder_and_file_and_gate() {
        let d = std::env::temp_dir().join("nova_seret_uji");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("a.txt"), "halo dunia\nbaris dua").unwrap();
        let (t, o) = info(&d.to_string_lossy());
        assert!(t.contains("1 berkas") && o.contains(&"Petakan".to_string()));
        assert!(is_dropped(&d.join("a.txt").to_string_lossy()));
        assert!(!is_dropped("C:/Windows/System32/cmd.exe"));
        let (t2, o2) = info(&d.join("a.txt").to_string_lossy());
        assert!(t2.contains("2 baris") && o2.contains(&"Ringkas".to_string()));
        assert!(resolve("Ringkas").unwrap().starts_with("ringkas "));
        assert!(resolve("tidak jelas").is_none());
        let _ = std::fs::remove_dir_all(&d);
    }
}
