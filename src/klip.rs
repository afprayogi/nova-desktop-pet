//! Pemantau clipboard KHUSUS saat Bos di editor/terminal (Mode Coding): bila yang disalin tampak seperti pesan error atau
//! potongan kode, NOVA (VOLT) menawarkan bantuan. Teks hanya ditahan di RAM (maks. 5 menit) sampai Bos menjawab;
//! tidak pernah ditulis ke disk. Teks yang tampak rahasia (kunci/sandi/token/kartu) diabaikan.
use crate::engine::Tx;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Error,
    Code,
}

struct Pending {
    text: String,
    kind: Kind,
    at: Instant,
}

static PENDING: Mutex<Option<Pending>> = Mutex::new(None);

#[cfg(windows)]
fn seq() -> u32 {
    unsafe { windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber() }
}

#[cfg(windows)]
fn read_text() -> Option<String> {
    use windows_sys::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard};
    use windows_sys::Win32::System::Memory::{GlobalLock, GlobalUnlock};
    use windows_sys::Win32::System::Ole::CF_UNICODETEXT;
    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return None;
        }
        let mut out = None;
        let h = GetClipboardData(CF_UNICODETEXT as u32);
        if !h.is_null() {
            let p = GlobalLock(h) as *const u16;
            if !p.is_null() {
                let mut n = 0;
                while n < 20_000 && *p.add(n) != 0 {
                    n += 1;
                }
                out = Some(String::from_utf16_lossy(std::slice::from_raw_parts(p, n)));
                GlobalUnlock(h);
            }
        }
        CloseClipboard();
        out
    }
}

#[cfg(not(windows))]
fn seq() -> u32 {
    0
}
#[cfg(not(windows))]
fn read_text() -> Option<String> {
    None
}

/// Teks yang tampak rahasia tidak diproses.
pub fn secretish(t: &str) -> bool {
    let l = t.to_lowercase();
    if ["-----begin", "password", "passwd", "kata sandi", "api_key", "apikey", "secret", "token", "bearer ", "authorization:", "private key", "aws_", "sk-"].iter().any(|k| l.contains(k)) {
        return true;
    }
    // Deretan panjang tanpa spasi berisi huruf+angka (kunci/hash) atau 13+ digit (kartu).
    t.split_whitespace().any(|w| w.len() >= 32 && w.chars().all(|c| c.is_ascii_alphanumeric() || "+/=_-".contains(c)))
        || t.chars().filter(|c| c.is_ascii_digit()).count() >= 13 && !t.contains(' ')
}

pub fn classify(t: &str) -> Option<Kind> {
    let t = t.trim();
    if t.len() < 12 || t.len() > 6000 || secretish(t) {
        return None;
    }
    let l = t.to_lowercase();
    if ["error", "exception", "traceback", "panicked", "failed", "cannot ", "undefined", "unresolved", "not found", "fatal", "segmentation", "warning:"].iter().any(|k| l.contains(k)) {
        return Some(Kind::Error);
    }
    let marks = ["{", "}", ";", "=>", "fn ", "def ", "function", "class ", "import ", "#include", "let ", "const ", "()", "return "];
    let hits = marks.iter().filter(|m| t.contains(*m)).count();
    (hits >= 3 || (hits >= 2 && t.contains('\n'))).then_some(Kind::Code)
}

pub fn language(t: &str) -> &'static str {
    let has = |ks: &[&str]| ks.iter().any(|k| t.contains(k));
    if has(&["fn ", "let mut", "impl ", "::<", "println!", "cargo", "error[E"]) {
        "Rust"
    } else if has(&["def ", "import ", "elif", "self.", "Traceback"]) && !has(&["{"]) {
        "Python"
    } else if has(&["#include", "std::", "int main", "printf("]) {
        "C/C++"
    } else if has(&["using System", "namespace ", "Console.Write"]) {
        "C#"
    } else if has(&["System.out", "public static void", "NullPointerException"]) {
        "Java"
    } else if has(&["package main", "func ", ":= "]) {
        "Go"
    } else if has(&["SELECT ", "INSERT INTO", "CREATE TABLE"]) {
        "SQL"
    } else if has(&["Write-Host", "Get-", "$_"]) {
        "PowerShell"
    } else if has(&["function", "=>", "const ", "console.", "undefined", "npm"]) {
        "JavaScript/TypeScript"
    } else {
        "bahasa tidak terdeteksi"
    }
}

fn text_for_match(t: &str) -> String {
    t.chars().take(3000).collect()
}

pub fn has_pending() -> bool {
    PENDING.lock().map(|p| p.as_ref().is_some_and(|x| x.at.elapsed() < Duration::from_secs(300))).unwrap_or(false)
}

pub fn clear() {
    if let Ok(mut p) = PENDING.lock() {
        *p = None;
    }
}

pub fn is_yes(t: &str) -> bool {
    let l = t.trim().to_lowercase();
    ["ya", "iya", "yes", "y", "ok", "oke", "boleh", "tolong", "bantu", "mau", "silakan", "lanjut", "yuk"].iter().any(|k| l == *k || l.starts_with(&format!("{k} ")))
}

pub fn is_no(t: &str) -> bool {
    let l = t.trim().to_lowercase();
    ["tidak", "nggak", "ga", "gak", "no", "nanti", "skip", "batal", "tak usah"].iter().any(|k| l == *k || l.starts_with(&format!("{k} ")))
}

/// Bahan jawaban dari memori hibrida (RAG) untuk teks yang disalin.
pub fn help() -> Option<Vec<(usize, String)>> {
    let p = PENDING.lock().ok()?.take()?;
    let lang = language(&p.text);
    let query: String = p.text.chars().take(400).collect();
    let docs = crate::ingat::hybrid(&format!("{lang} {query}"), 3);
    let mut lines = vec![(1, match p.kind {
        Kind::Error => format!("Ini tampak seperti pesan error ({lang}). Berikut yang kuingat yang paling relevan:"),
        Kind::Code => format!("Ini tampak seperti kode {lang}. Topik yang relevan di ingatanku:"),
    })];
    for d in &docs {
        lines.push((1, format!("• {}: {}", d.title, d.body.chars().take(220).collect::<String>())));
    }
    if docs.is_empty() {
        lines.push((1, "Belum ada catatan yang cocok. Ajari aku dokumentasinya: pelajari <file.pdf>.".into()));
    } else if let Some(first) = docs.first() {
        let rel = crate::ingat::related(&first.title, 3);
        if !rel.is_empty() {
            lines.push((3, format!("Terkait: {}.", rel.join(", "))));
        }
    }
    lines.push((0, "Kalau perlu penjelasan lebih dalam, hubungkan model (ketik /ollama) lalu tanya lagi.".into()));
    Some(lines)
}

pub async fn watch(tx: Tx) {
    let mut last = seq();
    let mut last_ask = Instant::now() - Duration::from_secs(600);
    loop {
        tokio::time::sleep(Duration::from_millis(1500)).await;
        let s = seq();
        if s == last {
            continue;
        }
        last = s;
        let (mode, idle) = crate::konteks::current();
        if mode != crate::konteks::Mode::Coding || idle > 120 || last_ask.elapsed() < Duration::from_secs(45) {
            continue;
        }
        let Some(text) = tokio::task::spawn_blocking(read_text).await.ok().flatten() else { continue };
        let Some(kind) = classify(&text) else { continue };
        let text_copy = text.clone();
        if let Ok(mut p) = PENDING.lock() {
            *p = Some(Pending { text, kind, at: Instant::now() });
        }
        let text = text_copy;
        last_ask = Instant::now();
        // Solusi lokal (regex) langsung ditampilkan bila cocok.
        if kind == Kind::Error {
            if let Some(sol) = crate::solusi::cari(&text_for_match(&text)) {
                crate::notify::toast("NOVA - solusi error", &sol);
                crate::adegan::play(&tx, vec![(1, format!("💡 {sol}")), (1, "Mau penjelasan lebih dalam dari ingatanku? Ketik: ya".into())]);
                tx.send(crate::engine::Event::Ask { question: "Penjelasan lebih dalam?".into(), options: vec!["ya".into(), "tidak".into()] });
                continue;
            }
        }
        let q = match kind {
            Kind::Error => "Aku lihat Bos menyalin pesan error. Butuh bantuan mencari penyebabnya? (ketik: ya / tidak)",
            Kind::Code => "Aku lihat Bos menyalin potongan kode. Mau kujelaskan atau kucarikan referensinya? (ketik: ya / tidak)",
        };
        crate::notify::toast("NOVA", "Butuh bantuan dengan yang barusan disalin?");
        crate::adegan::play(&tx, vec![(1, q.into())]);
        tx.send(crate::engine::Event::Ask { question: "Butuh bantuan?".into(), options: vec!["ya".into(), "tidak".into()] });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn classify_clipboard() {
        assert_eq!(classify("error[E0382]: borrow of moved value: `x`"), Some(Kind::Error));
        assert_eq!(classify("fn main() {\n    let x = 5;\n    println!(\"{}\", x);\n}"), Some(Kind::Code));
        assert_eq!(classify("api_key = sk-abcdef1234567890abcdef1234567890"), None);
        assert_eq!(classify("halo apa kabar hari ini"), None);
        assert_eq!(language("fn main() { let mut a = 1; }"), "Rust");
        assert!(is_yes("ya tolong") && is_no("nanti dulu") && !is_yes("tidak"));
    }
}
