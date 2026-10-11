//! Jurnal perubahan berkas oleh NOVA: setiap penulisan berkas dicatat dengan salinan cadangan sehingga bisa diurungkan
//! (`urungkan`). Berkas baru yang dibuat NOVA dihapus saat diurungkan (hanya berkas buatan NOVA sendiri).
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
struct Entry {
    ts: i64,
    path: String,
    /// salinan berkas lama; None = berkas ini baru dibuat NOVA
    backup: Option<String>,
}

fn dir() -> PathBuf {
    let d = crate::config::data_dir().join("jurnal");
    let _ = std::fs::create_dir_all(&d);
    d
}

fn log_path() -> PathBuf {
    dir().join("jurnal.json")
}

fn load() -> Vec<Entry> {
    std::fs::read_to_string(log_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save(v: &[Entry]) {
    let _ = std::fs::write(log_path(), serde_json::to_string(v).unwrap_or_default());
}

/// Catat sebelum menimpa/membuat `path`.
pub fn before_write(path: &str) {
    let p = PathBuf::from(path);
    let ts = chrono::Utc::now().timestamp();
    let backup = if p.is_file() {
        let b = dir().join(format!("{ts}_{}.bak", p.file_name().and_then(|n| n.to_str()).unwrap_or("berkas")));
        std::fs::copy(&p, &b).ok().map(|_| b.to_string_lossy().to_string())
    } else {
        None
    };
    let mut v = load();
    v.push(Entry { ts, path: path.to_string(), backup });
    while v.len() > 30 {
        let old = v.remove(0);
        if let Some(b) = old.backup {
            let _ = std::fs::remove_file(b);
        }
    }
    save(&v);
}

pub fn latest_ts() -> i64 {
    load().last().map(|e| e.ts).unwrap_or(0)
}

/// Urungkan perubahan terakhir. None bila jurnal kosong.
pub fn undo_last() -> Option<String> {
    let mut v = load();
    let e = v.pop()?;
    let msg = match &e.backup {
        Some(b) => match std::fs::copy(b, &e.path) {
            Ok(_) => format!("↩ Dikembalikan ke isi sebelumnya: {}", e.path),
            Err(err) => return Some(format!("Gagal mengembalikan {}: {err}", e.path)),
        },
        None => match std::fs::remove_file(&e.path) {
            Ok(_) => format!("↩ Berkas baru buatan NOVA dihapus: {}", e.path),
            Err(err) => return Some(format!("Gagal menghapus {}: {err}", e.path)),
        },
    };
    if let Some(b) = e.backup {
        let _ = std::fs::remove_file(b);
    }
    save(&v);
    Some(msg)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn write_then_undo_restores() {
        let f = std::env::temp_dir().join(format!("nova_jurnal_{}.txt", std::process::id()));
        std::fs::write(&f, "lama").unwrap();
        before_write(&f.to_string_lossy());
        std::fs::write(&f, "baru").unwrap();
        assert!(undo_last().unwrap().contains("Dikembalikan"));
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "lama");
        let _ = std::fs::remove_file(&f);
    }
}
