//! Saran tindakan yang BISA DILAKSANAKAN: NOVA menawarkan ("Tutup aplikasi menganggur?" / "Putar musik fokus?") dengan tombol Ok/Nanti.
//! Bila Bos menjawab ok/iya, perintahnya dijalankan seperti Bos mengetiknya sendiri (tindakan berisiko tetap meminta konfirmasi lagi).
//! Tiap jenis saran dinilai bandit (konteks x jenis) sehingga yang tidak disukai makin jarang muncul.
use crate::engine::{Event, Tx};
use std::sync::Mutex;
use std::time::{Duration, Instant};

struct Pending {
    cmd: String,
    kind: String,
    at: Instant,
}

static PENDING: Mutex<Option<Pending>> = Mutex::new(None);

/// Tawarkan saran. `cmd` = perintah obrolan yang dijalankan bila Bos menjawab ok.
/// Tawaran penting (mis. disk penuh): selalu ditampilkan, tidak dipengaruhi bandit/mode diam.
pub fn offer_always(tx: &Tx, text: &str, cmd: &str, kind: &str) {
    if let Ok(mut p) = PENDING.lock() {
        *p = Some(Pending { cmd: cmd.to_string(), kind: kind.to_string(), at: Instant::now() });
    }
    tx.send(Event::Ask { question: text.chars().take(110).collect(), options: vec!["Ok".into(), "Nanti".into()] });
}

pub fn offer(tx: &Tx, text: &str, cmd: &str, kind: &str) {
    if crate::bandit::quiet() || crate::adegan::level() == 0 {
        return;
    }
    let ctx = crate::bandit::ctx(&format!("saran:{kind}"));
    let a = crate::bandit::choose(&ctx, &["skip", "tawar"]);
    crate::bandit::expect(&ctx, &a);
    if a != "tawar" {
        return;
    }
    if let Ok(mut p) = PENDING.lock() {
        *p = Some(Pending { cmd: cmd.to_string(), kind: kind.to_string(), at: Instant::now() });
    }
    crate::notify::toast("NOVA - saran", text);
    tx.send(Event::Ask { question: text.chars().take(110).collect(), options: vec!["Ok".into(), "Nanti".into()] });
}

/// Jawaban Bos: Some(perintah) bila ok; None bila bukan jawaban saran (atau menolak, dicatat sebagai 👎).
pub fn resolve(answer: &str) -> Option<String> {
    let mut g = PENDING.lock().ok()?;
    let p = g.as_ref()?;
    if p.at.elapsed() > Duration::from_secs(900) {
        *g = None;
        return None;
    }
    if crate::klip::is_yes(answer) {
        let p = g.take()?;
        let _ = &p.kind;
        return Some(p.cmd);
    }
    if crate::klip::is_no(answer) {
        *g = None;
        crate::bandit::feedback(false);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn yes_returns_command_once() {
        *PENDING.lock().unwrap_or_else(|e| e.into_inner()) = Some(Pending { cmd: "rapikan folder download".into(), kind: "t".into(), at: Instant::now() });
        assert_eq!(resolve("ok").as_deref(), Some("rapikan folder download"));
        assert!(resolve("ok").is_none());
    }
}
