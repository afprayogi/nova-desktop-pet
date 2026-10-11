//! Adegan: agen (NOVA, VOLT, SENTRY, MEMO) berbicara. Isi disusun dari data nyata tanpa model.
//! Pengendali kecerewetan (`/cerewet 0|1|2`, bawaan 1): 0 = hanya hal penting, 1 = berguna & jarang, 2 = ramai (obrolan antar agen).
//! Pesan proaktif "lunak" tidak muncul saat Bos baru saja mengobrol dan dibatasi jedanya.
use crate::engine::{ChatLine, Event, Tx, Who};
use rand::seq::SliceRandom;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const NOVA: usize = 0;
const VOLT: usize = 1;
const SENTRY: usize = 2;
const MEMO: usize = 3;

pub type Line = (usize, String);

static LAST_SOFT: Mutex<Option<Instant>> = Mutex::new(None);
static LAST_CHAT: Mutex<Option<Instant>> = Mutex::new(None);

fn cut(s: &str, n: usize) -> String {
    let t: String = s.chars().take(n).collect();
    if s.chars().count() > n { format!("{t}…") } else { t }
}

fn level_file() -> std::path::PathBuf {
    crate::config::data_dir().join("cerewet.txt")
}

/// 0 diam (hanya penting) · 1 normal (bawaan) · 2 ramai.
pub fn level() -> u8 {
    std::fs::read_to_string(level_file()).ok().and_then(|t| t.trim().parse().ok()).filter(|n: &u8| *n <= 2).unwrap_or(1)
}

pub fn set_level(n: u8) {
    let _ = std::fs::write(level_file(), n.min(2).to_string());
}

/// Dipanggil saat Bos mengirim pesan: pesan proaktif ditahan beberapa menit.
pub fn note_chat() {
    if let Ok(mut l) = LAST_CHAT.lock() {
        *l = Some(Instant::now());
    }
}

/// Bolehkah pesan proaktif lunak tampil sekarang? Menandai waktu bila ya.
pub fn soft_ok(min_level: u8, gap_min: u64) -> bool {
    if level() < min_level {
        return false;
    }
    if LAST_CHAT.lock().ok().and_then(|l| *l).is_some_and(|t| t.elapsed() < Duration::from_secs(180)) {
        return false;
    }
    if crate::ambien::typing() || crate::ambien::media() == "video" {
        return false; // jangan ganggu saat mengetik atau menonton
    }
    let mut g = LAST_SOFT.lock().unwrap_or_else(|e| e.into_inner());
    if g.is_some_and(|t| t.elapsed() < Duration::from_secs(gap_min * 60)) {
        return false;
    }
    *g = Some(Instant::now());
    true
}

/// Pemberitahuan proaktif: bubble di peliharaan + daftar Notifikasi. TIDAK masuk obrolan (obrolan hanya percakapan Bos).
pub fn play(tx: &Tx, lines: Vec<Line>) {
    let lines: Vec<Line> = lines.into_iter().map(|(_, t)| (0, t)).collect(); // satu orang saja: NOVA
    if lines.is_empty() {
        return;
    }
    tx.send(Event::Scene(lines));
}

/// Pelengkap jawaban atas pertanyaan Bos (mis. peringatan keselamatan): tampil juga di obrolan.
pub fn play_reply(tx: &Tx, lines: Vec<Line>) {
    let lines: Vec<Line> = lines.into_iter().map(|(_, t)| (0, t)).collect();
    if lines.is_empty() {
        return;
    }
    for (i, t) in &lines {
        tx.send(Event::Chat(ChatLine { who: Who::Agent(*i), text: t.clone(), media: Vec::new() }));
    }
    tx.send(Event::Scene(lines));
}

/// Versi lunak: hanya bila tingkat kecerewetan & jeda mengizinkan.
pub fn play_soft(tx: &Tx, lines: Vec<Line>, min_level: u8, gap_min: u64) {
    if soft_ok(min_level, gap_min) {
        play(tx, lines);
    }
}

fn health_short() -> String {
    crate::monitor::summary().split(" | ").take(3).collect::<Vec<_>>().join(" · ")
}

/// Pengingat terjadwal: singkat (satu atau dua baris).
pub fn on_nudge(tx: &Tx, text: &str) {
    let l = text.to_lowercase();
    let mut v: Vec<Line> = vec![(NOVA, text.to_string())];
    if l.contains("tidur") || l.contains("wind-down") {
        let sleep = crate::assistant::report("tidur");
        v.push((MEMO, cut(sleep.lines().last().unwrap_or("Target tidur 8 jam."), 120)));
    } else if l.contains("non-stop") || l.contains("istirahat") {
        v.push((VOLT, "Regangkan bahu, tatap objek jauh 20 detik, minum air putih.".into()));
    }
    play(tx, v);
}

pub fn on_alert(tx: &Tx, text: &str, top: &str) {
    let mut v = vec![(SENTRY, text.to_string())];
    if !top.is_empty() {
        v.push((NOVA, format!("Proses terberat: {top}.")));
    }
    play(tx, v);
}

/// Setelah NOVA menjawab pertanyaan teknik: hanya peringatan keselamatan (dan sumber bila level ramai).
pub fn on_answer(tx: &Tx, question: &str, answer: &str) {
    let docs = crate::ingat::hybrid(question, 1);
    let mut v: Vec<Line> = vec![];
    if level() >= 2 {
        if let Some(d) = docs.first() {
            let rel = crate::ingat::related(&d.title, 3);
            v.push((MEMO, format!("Sumber: {} [{}]{}", d.title, d.src, if rel.is_empty() { String::new() } else { format!(". Terkait: {}", rel.join(", ")) })));
        }
    }
    if answer.contains("Verifikasi") {
        v.push((SENTRY, "⚠ Angka keselamatan: verifikasi ke PUIL/SNI sebelum dipakai di lapangan.".into()));
    }
    play_reply(tx, v);
}

/// Obrolan santai antar agen (hanya tingkat 2).
pub fn idle(tx: &Tx) {
    if level() < 2 {
        return;
    }
    let Ok(c) = crate::teknik::open() else { return };
    let Ok(title) = c.query_row("SELECT title FROM kb WHERE src='bawaan' ORDER BY RANDOM() LIMIT 1", [], |r| r.get::<_, String>(0)) else { return };
    let docs = crate::ingat::hybrid(&title, 1);
    let Some(d) = docs.first() else { return };
    let rel = crate::ingat::related(&d.title, 3);
    let mut rng = rand::thread_rng();
    let mut v: Vec<Line> = vec![(VOLT, format!("Tahukah Bos? {}: {}", d.title, cut(&d.body, 130)))];
    if !rel.is_empty() {
        v.push((MEMO, format!("Terkait di grafku: {}.", rel.join(", "))));
    }
    if rand::random::<u8>() % 10 < 4 {
        v.push((VOLT, crate::meme::meme(&d.title, rand::random::<usize>() % 7)));
    }
    let _ = (&mut rng, ["Menarik!"].choose(&mut rand::thread_rng()));
    play_soft(tx, v, 2, 8);
}

/// Salam saat aplikasi mulai: dua baris, tanpa basa-basi.
pub fn greet(tx: &Tx, booted: bool) {
    if level() == 0 {
        return;
    }
    let first = crate::assistant::report("agenda").lines().next().map(|s| s.to_string()).filter(|s| !s.trim().is_empty()).unwrap_or_else(|| health_short());
    let first = if first.chars().next().is_some_and(|c| c.is_ascii_digit()) { format!("Jadwal berikutnya pukul {first}") } else { first };
    let v = vec![(NOVA, if booted { format!("Selamat {}, Bos! Laptop baru menyala.", crate::sistem::salam()) } else { format!("Selamat {}, Bos! Aku aktif.", crate::sistem::salam()) }), (MEMO, cut(&first, 120))];
    play(tx, v);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cut_works() {
        assert_eq!(cut("abcdef", 3), "abc…");
        assert_eq!(cut("ab", 3), "ab");
    }
    #[test]
    fn soft_messages_are_rate_limited() {
        // gap besar: panggilan kedua harus ditolak
        let first = soft_ok(0, 60);
        let second = soft_ok(0, 60);
        assert!(!(first && second));
    }
}
