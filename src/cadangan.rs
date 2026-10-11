//! Cadangan harian otomatis data NOVA (memori, jadwal, pola, pengaturan). Disimpan di D:\NOVA-backup bila drive D ada
//! (C: sering penuh), jika tidak di folder data. Basis data disalin konsisten dengan VACUUM INTO; 7 hari terakhir dipertahankan.
use std::path::PathBuf;

fn target_root() -> PathBuf {
    if cfg!(windows) && std::path::Path::new("D:/").exists() {
        PathBuf::from("D:/NOVA-backup")
    } else {
        crate::config::data_dir().join("backup")
    }
}

/// Jalankan cadangan hari ini bila belum ada. Mengembalikan jumlah berkas yang dicadangkan (0 = sudah ada/gagal).
pub fn run() -> usize {
    let src = crate::config::data_dir();
    let root = target_root();
    let day = chrono::Local::now().format("%Y-%m-%d").to_string();
    let dir = root.join(&day);
    if dir.exists() || std::fs::create_dir_all(&dir).is_err() {
        return 0;
    }
    let mut n = 0;
    if let Ok(rd) = std::fs::read_dir(&src) {
        for e in rd.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("");
            let dest = dir.join(&name);
            if ext == "db" && name != "indeks.db" {
                // indeks berkas besar & bisa dibangun ulang: tidak dicadangkan
                if let Ok(c) = rusqlite::Connection::open(&p) {
                    if c.execute("VACUUM INTO ?1", [dest.to_string_lossy().to_string()]).is_ok() {
                        n += 1;
                    }
                }
            } else if ["json", "ics", "txt"].contains(&ext) && name != "api.token" && !name.contains("key") {
                // kunci & token sengaja tidak disalin
                if std::fs::copy(&p, &dest).is_ok() {
                    n += 1;
                }
            }
        }
    }
    // pertahankan 7 hari terakhir
    if let Ok(rd) = std::fs::read_dir(&root) {
        let mut days: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_dir() && p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.len() == 10 && n.as_bytes()[4] == b'-' && n.as_bytes()[7] == b'-')).collect();
        days.sort();
        while days.len() > 7 {
            let _ = std::fs::remove_dir_all(days.remove(0));
        }
    }
    n
}
