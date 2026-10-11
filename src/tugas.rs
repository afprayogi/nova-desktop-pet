//! Tugas utama harian yang interaktif: NOVA MEMINTA 1 tugas utama saat jam kerja mulai, mengingatnya, lalu menanyakan hasilnya
//! di akhir blok kerja (jawaban lewat tombol Sudah/Belum). Tersimpan di assistant.db dan dipakai di briefing/agenda.
use crate::engine::{Event, Tx};
use chrono::Local;
use std::sync::atomic::{AtomicBool, Ordering};

static AWAIT_DONE: AtomicBool = AtomicBool::new(false);

fn db() -> Option<rusqlite::Connection> {
    let c = crate::assistant::db().ok()?;
    let _ = c.execute_batch("CREATE TABLE IF NOT EXISTS tugas(day TEXT PRIMARY KEY, text TEXT, done INT DEFAULT 0);");
    Some(c)
}

fn today_key() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

/// (teks, selesai?) untuk hari ini.
pub fn today() -> Option<(String, bool)> {
    db()?.query_row("SELECT text, done FROM tugas WHERE day=?1", [today_key()], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? != 0))).ok()
}

pub fn set(text: &str) -> String {
    let t: String = text.trim().trim_matches(|c| c == ':' || c == '"').trim().chars().take(140).collect();
    if t.is_empty() {
        return match today() {
            Some((x, true)) => format!("Tugas utama hari ini: {x} ✅ (selesai)"),
            Some((x, false)) => format!("Tugas utama hari ini: {x} (belum selesai). Ketik 'tugas utama selesai' bila sudah."),
            None => "Belum ada tugas utama hari ini. Ketik: tugas utama <apa yang paling penting hari ini>".into(),
        };
    }
    if matches!(t.to_lowercase().as_str(), "selesai" | "sudah" | "done") {
        return mark_done();
    }
    if let Some(c) = db() {
        let _ = c.execute("INSERT OR REPLACE INTO tugas VALUES(?1,?2,0)", (today_key(), &t));
    }
    crate::ingat::episode("tugas", &t);
    format!("✅ Tugas utama hari ini: {t}. Kuingatkan lagi saat istirahat siang dan sore.")
}

pub fn mark_done() -> String {
    AWAIT_DONE.store(false, Ordering::Relaxed);
    match today() {
        Some((x, _)) => {
            if let Some(c) = db() {
                let _ = c.execute("UPDATE tugas SET done=1 WHERE day=?1", [today_key()]);
            }
            format!("🎉 Mantap! Tugas utama '{x}' selesai. Satu langkah besar hari ini.")
        }
        None => "Belum ada tugas utama yang kucatat hari ini.".into(),
    }
}

pub fn awaiting_done() -> bool {
    AWAIT_DONE.load(Ordering::Relaxed)
}

pub fn clear_await() {
    AWAIT_DONE.store(false, Ordering::Relaxed);
}

/// Jam kerja mulai: minta Bos menentukan 1 tugas utama (jawaban bebas diketik di obrolan).
pub fn ask_main(tx: &Tx) {
    crate::ml::dst_set_force("tugas utama");
    crate::notify::toast("NOVA", "Apa 1 tugas utama hari ini?");
    crate::adegan::play(tx, vec![(0, "Jam kerja hampir mulai. Apa 1 tugas utama yang paling penting hari ini? Ketik jawabanmu di bawah.".into())]);
    tx.send(Event::Ask { question: "Tugas utama hari ini?".into(), options: vec!["Belum tahu, nanti".into()] });
}

/// Akhir blok kerja: tanya apakah tugas utama sudah selesai.
pub fn check_done(tx: &Tx) {
    let Some((t, false)) = today() else { return };
    AWAIT_DONE.store(true, Ordering::Relaxed);
    crate::notify::toast("NOVA", &format!("Tugas utama '{t}' sudah selesai?"));
    crate::adegan::play(tx, vec![(3, format!("Tugas utama hari ini: \"{t}\". Sudah selesai?"))]);
    tx.send(Event::Ask { question: "Tugas utama sudah selesai?".into(), options: vec!["Sudah".into(), "Belum".into()] });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn set_show_and_finish() {
        let c = db().unwrap();
        let _ = c.execute("DELETE FROM tugas WHERE day=?1", [today_key()]);
        assert!(set("").contains("Belum ada"));
        assert!(set("Selesaikan laporan motor").contains("Tugas utama hari ini"));
        assert_eq!(today(), Some(("Selesaikan laporan motor".to_string(), false)));
        assert!(set("selesai").contains("selesai"));
        assert_eq!(today().map(|t| t.1), Some(true));
        let _ = c.execute("DELETE FROM tugas WHERE day=?1", [today_key()]);
    }
}
