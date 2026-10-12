//! Data NOVA yang bisa diedit tangan: kamus aksi, alat CLI, dan layanan web disimpan sebagai berkas JSON di folder data
//! (`%APPDATA%\nova`). Bila berkas belum ada, dibuat dari bawaan; perubahan terbaca otomatis tanpa memulai ulang (dicek lewat waktu ubah).
//! JSON yang rusak tidak membuat NOVA berhenti: dipakai bawaan, dan galatnya ditampilkan oleh `/data`.
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

pub const BERKAS: &[&str] = &["kamus_aksi.json", "cli.json", "layanan.json", "ajar.json", "putusan.json", "kasus.json", "penasaran.json"];

type Isi = (Option<SystemTime>, Arc<Value>, String);
static CACHE: Mutex<Option<HashMap<String, Isi>>> = Mutex::new(None);

fn path(nama: &str) -> std::path::PathBuf {
    crate::config::otak_dir().join(nama)
}

fn mtime(nama: &str) -> Option<SystemTime> {
    std::fs::metadata(path(nama)).and_then(|m| m.modified()).ok()
}

/// Isi berkas JSON `nama`; dibuat dari `bawaan` bila belum ada; kembali ke `bawaan` bila rusak.
pub fn json(nama: &str, bawaan: &str) -> Arc<Value> {
    let mt = mtime(nama);
    let mut g = CACHE.lock().unwrap();
    let m = g.get_or_insert_with(HashMap::new);
    if let Some((t, v, _)) = m.get(nama) {
        if *t == mt && mt.is_some() {
            return v.clone();
        }
    }
    if mt.is_none() {
        let _ = std::fs::write(path(nama), bawaan);
    }
    let (v, galat) = match std::fs::read_to_string(path(nama)).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str::<Value>(&t).map_err(|e| e.to_string())) {
        Ok(v) => (v, String::new()),
        Err(e) => (serde_json::from_str(bawaan).unwrap_or(Value::Null), e),
    };
    let v = Arc::new(v);
    m.insert(nama.to_string(), (mtime(nama), v.clone(), galat));
    v
}

/// Ubah berkas lewat `f` (mulai dari isi sekarang), lalu tulis kembali rapi. Mengembalikan galat penulisan bila ada.
pub fn ubah(nama: &str, bawaan: &str, f: impl FnOnce(&mut Value)) -> Result<(), String> {
    let mut v = (*json(nama, bawaan)).clone();
    f(&mut v);
    let r = std::fs::write(path(nama), serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?).map_err(|e| e.to_string());
    // timestamp berkas bisa sama dengan pembacaan sebelumnya (resolusi kasar): paksa baca ulang
    if let Some(m) = CACHE.lock().unwrap().as_mut() {
        m.remove(nama);
    }
    r
}

/// Ringkasan status semua berkas data (untuk `/data`).
pub fn periksa() -> String {
    // pastikan berkas bawaan sudah dibuat
    let _ = (crate::kata::aksi("x"), crate::cli::daftar(), crate::aturan::layanan_daftar());
    let g = CACHE.lock().unwrap();
    let mut s = format!("Folder otak: {}\n", crate::config::otak_dir().display());
    for n in BERKAS {
        let ada = path(n).exists();
        let info = g.as_ref().and_then(|m| m.get(*n));
        s.push_str(&match (ada, info) {
            (false, _) => format!("• {n}: belum ada\n"),
            (true, Some((_, _, e))) if !e.is_empty() => format!("• {n}: RUSAK ({e}); memakai bawaan sampai diperbaiki\n"),
            (true, Some((_, v, _))) => format!("• {n}: ok ({} entri)\n", v["layanan"].as_array().or_else(|| v.as_array()).map(|a| a.len()).or_else(|| v.as_object().map(|o| o.len())).unwrap_or(0)),
            (true, None) => format!("• {n}: ada\n"),
        });
    }
    s.push_str("Edit berkas itu langsung, atau minta aku menambahkannya (mis. /kamusaksi tambah putar dengerin).");
    s
}

pub fn folder() -> std::path::PathBuf {
    crate::config::otak_dir()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn creates_default_reloads_and_survives_broken_json() {
        let n = format!("uji_data_{}.json", std::process::id());
        assert_eq!(json(&n, r#"{"a":1}"#)["a"], 1);
        assert!(path(&n).exists());
        ubah(&n, r#"{"a":1}"#, |v| v["a"] = 2.into()).unwrap();
        assert_eq!(json(&n, r#"{"a":1}"#)["a"], 2);
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(path(&n), "{rusak").unwrap();
        assert_eq!(json(&n, r#"{"a":1}"#)["a"], 1); // jatuh ke bawaan
        let _ = std::fs::remove_file(path(&n));
    }
}
