//! Log audit tindakan alat (kapan, apa, hasil ringkas), ekspor data pribadi, dan hapus data pribadi. Semua lokal.
//! Argumen alat yang memuat rahasia disamarkan; log dipangkas otomatis agar tidak membengkak.
use crate::alat::Call;
use crate::config::data_dir;
use chrono::Local;
use serde_json::{json, Value};
use std::io::Write;

const MAKS_BYTE: u64 = 1_000_000;
/// Alat yang argumennya bisa berisi kata sandi/kunci/isi surat: argumen dicatat sebagai ***.
const RAHASIA: &[&str] = &["password", "email", "koneksi", "gemini", "catat", "catatan", "tulis"];
/// Alat baca-saja yang sangat sering dipanggil dan tidak berguna untuk audit.
const DIABAIKAN: &[&str] = &["info", "waktu", "baterai", "health", "ps"];

fn berkas() -> std::path::PathBuf {
    data_dir().join("audit.jsonl")
}

pub fn catat(c: &Call, hasil: &str) {
    if DIABAIKAN.contains(&c.tool.as_str()) {
        return;
    }
    let arg = if RAHASIA.contains(&c.tool.as_str()) { "***".to_string() } else { c.arg.lines().next().unwrap_or("").chars().take(160).collect() };
    let baris = json!({"t": Local::now().to_rfc3339(), "alat": c.tool, "arg": arg, "hasil": hasil.lines().next().unwrap_or("").chars().take(120).collect::<String>()});
    let p = berkas();
    if p.metadata().map(|m| m.len() > MAKS_BYTE).unwrap_or(false) {
        // buang separuh tertua
        if let Ok(isi) = std::fs::read_to_string(&p) {
            let sisa: Vec<&str> = isi.lines().collect();
            let _ = std::fs::write(&p, sisa[sisa.len() / 2..].join("\n") + "\n");
        }
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&p) {
        let _ = writeln!(f, "{baris}");
    }
}

pub fn terbaru(n: usize, kata: &str) -> Vec<Value> {
    let Ok(isi) = std::fs::read_to_string(berkas()) else { return vec![] };
    let k = kata.to_lowercase();
    isi.lines().rev().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|v| k.is_empty() || v["alat"].as_str().unwrap_or("").contains(&k) || v["arg"].as_str().unwrap_or("").to_lowercase().contains(&k)).take(n).collect()
}

fn log(arg: &str) -> String {
    let (n, kata) = match arg.trim().parse::<usize>() {
        Ok(n) => (n.clamp(1, 50), ""),
        Err(_) => (15, arg.trim()),
    };
    let v = terbaru(n, kata);
    if v.is_empty() {
        return "Belum ada tindakan tercatat.".into();
    }
    format!("🧾 Tindakan terakhir{}:\n{}", if kata.is_empty() { String::new() } else { format!(" ('{kata}')") }, v.iter().map(|x| format!("{} · {} {} → {}", x["t"].as_str().unwrap_or("").get(5..16).unwrap_or("").replace('T', " "), x["alat"].as_str().unwrap_or(""), x["arg"].as_str().unwrap_or(""), x["hasil"].as_str().unwrap_or(""))).collect::<Vec<_>>().join("\n"))
}

/// Berkas yang tidak pernah diekspor: kunci, token, dan kredensial.
fn rahasia(nama: &str) -> bool {
    let n = nama.to_lowercase();
    n.ends_with(".key") || n.contains("token") || n.contains("rahasia") || n.contains("secret") || n.contains("oauth") || n.contains("kredensial") || n.ends_with(".pem") || n.contains("password")
}

fn ekspor() -> String {
    let tujuan = std::env::var("USERPROFILE").map(|p| std::path::PathBuf::from(p).join("Documents").join("NOVA")).unwrap_or_else(|_| data_dir()).join(format!("ekspor-{}", Local::now().format("%Y%m%d-%H%M")));
    if std::fs::create_dir_all(&tujuan).is_err() {
        return "Gagal membuat folder ekspor.".into();
    }
    let (mut n, mut lewat) = (0, 0);
    if let Ok(rd) = std::fs::read_dir(data_dir()) {
        for e in rd.flatten() {
            let p = e.path();
            let nama = e.file_name().to_string_lossy().into_owned();
            if !p.is_file() || e.metadata().map(|m| m.len() > 100_000_000).unwrap_or(true) {
                continue;
            }
            if rahasia(&nama) {
                lewat += 1;
            } else if std::fs::copy(&p, tujuan.join(&nama)).is_ok() {
                n += 1;
            }
        }
    }
    format!("📦 {n} berkas data disalin ke {} ({lewat} berkas kunci/token sengaja tidak disertakan). Berkas basis data (.db) dapat dibuka dengan SQLite.", tujuan.display())
}

/// Hapus data pribadi (riwayat aktivitas, memori, kesehatan, jurnal, audit); pengaturan, kunci, dan tugas terjadwal tidak disentuh.
fn hapus() -> String {
    let (mut ok, mut gagal) = (vec![], vec![]);
    for f in ["assistant.db", "kesehatan.db", "memori.db", "indeks.db", "dompet.json", "buddy.json", "makan.json", "audit.jsonl", "waktu_proyek.json", "tugas_hasil.jsonl", "tugas_pesan.jsonl"] {
        let p = data_dir().join(f);
        if p.exists() {
            match std::fs::remove_file(&p) {
                Ok(()) => ok.push(f),
                Err(_) => gagal.push(f),
            }
        }
    }
    if std::fs::remove_dir_all(data_dir().join("jurnal")).is_ok() {
        ok.push("jurnal/");
    }
    if let Ok(c) = crate::teknik::open() {
        let _ = c.execute("DELETE FROM ep", []);
    }
    crate::ingat::clear_cache();
    format!("🗑 Terhapus: {}.{}", if ok.is_empty() { "tidak ada".into() } else { ok.join(", ") }, if gagal.is_empty() { String::new() } else { format!(" Masih dipakai NOVA (hapus setelah keluar dari NOVA): {}.", gagal.join(", ")) })
}

/// Perintah alat `privasi`: log [n|kata] | ekspor | hapus
pub fn jalankan(arg: &str) -> String {
    let (cmd, rest) = arg.trim().split_once(' ').map(|(a, b)| (a, b.trim())).unwrap_or((arg.trim(), ""));
    match cmd {
        "ekspor" => ekspor(),
        "hapus" => hapus(),
        _ => log(rest),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    static K: std::sync::Mutex<()> = std::sync::Mutex::new(()); // hapus() menyentuh audit.jsonl

    #[test]
    fn audit_menyamarkan_rahasia_dan_memfilter() {
        let _k = K.lock().unwrap_or_else(|e| e.into_inner());
        catat(&Call { tool: "password".into(), arg: "hunter2".into() }, "ok");
        catat(&Call { tool: "cuaca".into(), arg: "Bandung".into() }, "28°C\nbaris dua");
        let semua = terbaru(10, "");
        assert!(semua.iter().any(|v| v["alat"] == "password" && v["arg"] == "***"));
        assert!(!serde_json::to_string(&semua).unwrap().contains("hunter2"));
        assert_eq!(terbaru(10, "bandung").len(), 1);
        assert!(log("").contains("cuaca"));
    }

    #[test]
    fn berkas_rahasia_tidak_diekspor() {
        for n in ["api.token", "gemini.key", "google_oauth.json", "rahasia.txt"] {
            assert!(rahasia(n), "{n}");
        }
        assert!(!rahasia("keahlian.json"));
    }

    #[test]
    fn hapus_dan_ekspor_di_folder_uji() {
        let _k = K.lock().unwrap_or_else(|e| e.into_inner());
        std::fs::write(data_dir().join("buddy.json"), "{}").unwrap();
        std::fs::write(data_dir().join("api.token"), "t").unwrap();
        assert!(hapus().contains("buddy.json"));
        assert!(data_dir().join("api.token").exists());
        assert!(!data_dir().join("buddy.json").exists());
    }
}
