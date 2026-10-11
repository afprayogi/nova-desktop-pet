//! Kesadaran layar: tangkap layar -> OCR dengan Tesseract LOKAL (tidak ada data keluar dari laptop).
//! Privasi: (1) baca-layar sesuai perintah Bos selalu minta izin; (2) mode berkala MATI secara bawaan, dinyalakan dari
//! menu tray, dan hanya menyimpan kata kunci di RAM (bukan teks), melewati layar yang terlihat sensitif;
//! (3) file tangkapan sementara langsung dihapus.
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

static ENABLED: AtomicBool = AtomicBool::new(false);
static KEYWORDS: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn flag_file() -> PathBuf {
    crate::config::data_dir().join("layar.on")
}

pub fn load_flag() {
    ENABLED.store(flag_file().exists(), Ordering::Relaxed);
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
    if on {
        let _ = std::fs::write(flag_file(), "1");
    } else {
        let _ = std::fs::remove_file(flag_file());
        if let Ok(mut k) = KEYWORDS.lock() {
            k.clear();
        }
    }
}

/// Cari tesseract: env TESSERACT, file tesseract.txt di folder data, PATH, lalu lokasi umum.
pub fn find_tesseract() -> Option<PathBuf> {
    let mut cands: Vec<PathBuf> = vec![];
    if let Ok(p) = std::env::var("TESSERACT") {
        cands.push(p.into());
    }
    if let Ok(t) = std::fs::read_to_string(crate::config::data_dir().join("tesseract.txt")) {
        cands.push(t.trim().trim_matches('"').into());
    }
    if let Some(paths) = std::env::var_os("PATH") {
        cands.extend(std::env::split_paths(&paths).map(|d| d.join("tesseract.exe")));
        cands.extend(std::env::split_paths(&paths).map(|d| d.join("tesseract")));
    }
    for d in [r"C:\Program Files\Tesseract-OCR", r"C:\Program Files (x86)\Tesseract-OCR", r"C:\ProgramData\chocolatey\bin"] {
        cands.push(PathBuf::from(d).join("tesseract.exe"));
    }
    if let Some(l) = dirs::data_local_dir() {
        cands.push(l.join("Programs").join("Tesseract-OCR").join("tesseract.exe"));
    }
    if let Some(h) = dirs::home_dir() {
        cands.push(h.join("scoop").join("apps").join("tesseract").join("current").join("tesseract.exe"));
    }
    cands.into_iter().find(|p| p.is_file())
}

pub const NOT_FOUND: &str = "Tesseract tidak ditemukan. Pasang (winget install UB-Mannheim.TesseractOCR) atau tulis path tesseract.exe ke file tesseract.txt di folder data NOVA.";

/// Tangkap layar utama ke PNG (PowerShell + System.Drawing, tanpa pustaka tambahan).
#[cfg(windows)]
fn capture(out: &std::path::Path) -> Result<(), String> {
    let ps = format!(
        "Add-Type -AssemblyName System.Windows.Forms,System.Drawing; \
         $b=[System.Windows.Forms.SystemInformation]::VirtualScreen; \
         $bmp=New-Object System.Drawing.Bitmap $b.Width,$b.Height; \
         $g=[System.Drawing.Graphics]::FromImage($bmp); \
         $g.CopyFromScreen($b.Left,$b.Top,0,0,$bmp.Size); \
         $bmp.Save('{}',[System.Drawing.Imaging.ImageFormat]::Png); $g.Dispose(); $bmp.Dispose()",
        out.display()
    );
    let o = Command::new("powershell").args(["-NoProfile", "-NonInteractive", "-Command", &ps]).stdout(Stdio::null()).output().map_err(|e| e.to_string())?;
    if o.status.success() && out.exists() { Ok(()) } else { Err("gagal menangkap layar".into()) }
}

#[cfg(not(windows))]
fn capture(_out: &std::path::Path) -> Result<(), String> {
    Err("tangkap layar baru didukung di Windows".into())
}

fn run_ocr(exe: &std::path::Path, img: &std::path::Path) -> Result<String, String> {
    for lang in ["ind+eng", "eng"] {
        let o = Command::new(exe)
            .arg(img)
            .arg("stdout")
            .args(["-l", lang])
            .env("OMP_THREAD_LIMIT", "1") // hemat CPU di laptop lemah
            .stdin(Stdio::null())
            .output()
            .map_err(|e| e.to_string())?;
        if o.status.success() {
            return Ok(String::from_utf8_lossy(&o.stdout).into_owned());
        }
    }
    Err("OCR gagal (cek paket bahasa tessdata)".into())
}

/// Tangkap + OCR; file sementara selalu dihapus.
pub fn read_screen() -> Result<String, String> {
    let exe = find_tesseract().ok_or(NOT_FOUND)?;
    let img = std::env::temp_dir().join(format!("nova-layar-{}.png", std::process::id()));
    let r = capture(&img).and_then(|_| run_ocr(&exe, &img));
    let _ = std::fs::remove_file(&img);
    r.map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// Layar yang tampak sensitif (sandi, kartu, token) tidak diproses sama sekali.
pub fn sensitive(text: &str) -> bool {
    let l = text.to_lowercase();
    if ["password", "kata sandi", "sandi", "cvv", "cvc", "pin atm", "otp", "private key", "secret", "token", "api key", "login", "sign in", "masuk"].iter().any(|k| l.contains(k)) {
        return true;
    }
    // Deretan 13-19 digit (kartu/rekening).
    let digits: String = l.chars().filter(|c| c.is_ascii_digit() || *c == ' ' || *c == '-').collect();
    digits.split(|c: char| !c.is_ascii_digit() && c != ' ' && c != '-').any(|s| s.chars().filter(|c| c.is_ascii_digit()).count() >= 13)
}

/// Kata kunci teratas (bukan teks asli) untuk konteks.
pub fn extract_keywords(text: &str, n: usize) -> Vec<String> {
    let mut f: std::collections::HashMap<String, u32> = Default::default();
    for w in crate::teknik::terms(text) {
        if w.len() > 3 && !w.chars().all(|c| c.is_ascii_digit()) {
            *f.entry(w).or_default() += 1;
        }
    }
    let mut v: Vec<_> = f.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v.into_iter().take(n).map(|x| x.0).collect()
}

fn has_error(text: &str) -> bool {
    let l = text.to_lowercase();
    ["error[", "exception", "traceback", "panicked", "failed to", "fatal", "cannot find", "undefined reference", "segmentation fault"].iter().any(|k| l.contains(k))
}

/// Tugas berkala (hanya bila diaktifkan Bos): tiap 3 menit saat Bos aktif.
pub async fn periodic(tx: crate::engine::Tx) {
    load_flag();
    let mut warned_missing = false;
    loop {
        tokio::time::sleep(Duration::from_secs(180)).await;
        load_flag();
        if !enabled() || crate::konteks::IDLE_SECS.load(Ordering::Relaxed) > 60 {
            continue;
        }
        if find_tesseract().is_none() {
            if !warned_missing {
                tx.sys(format!("🖥 Kesadaran layar aktif, tetapi {NOT_FOUND}"));
                warned_missing = true;
            }
            continue;
        }
        let text = match tokio::task::spawn_blocking(read_screen).await {
            Ok(Ok(t)) => t,
            _ => continue,
        };
        if sensitive(&text) {
            if let Ok(mut k) = KEYWORDS.lock() {
                k.clear();
            }
            continue; // teks dibuang, tidak disimpan
        }
        let kw = extract_keywords(&text, 6);
        crate::ingat::work_push(&format!("layar: {}", kw.join(", ")));
        if let Ok(mut k) = KEYWORDS.lock() {
            *k = kw;
        }
        if has_error(&text) && crate::konteks::current().0 == crate::konteks::Mode::Coding {
            crate::adegan::play(&tx, vec![(1, "Ada pesan error di layar. Salin barisnya lalu ketik: tanya <pesan error>, atau 'baca layar' agar kubantu baca.".into())]);
        }
        drop(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sensitivity_and_keywords() {
        assert!(sensitive("Masukkan password Anda"));
        assert!(sensitive("Nomor kartu 4111 1111 1111 1111"));
        assert!(!sensitive("Hukum Ohm menyatakan tegangan sama dengan arus kali hambatan"));
        let k = extract_keywords("tegangan tegangan arus hambatan hambatan hambatan 12345", 2);
        assert_eq!(k[0], "hambatan");
    }
    #[test]
    fn error_detection() {
        assert!(has_error("thread main panicked at src/main.rs"));
        assert!(!has_error("semua baik"));
    }
}
