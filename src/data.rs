//! Data NOVA yang bisa diedit tangan: kamus aksi, alat CLI, dan layanan web disimpan sebagai berkas JSON di folder data
//! (`%APPDATA%\nova`). Bila berkas belum ada, dibuat dari bawaan; perubahan terbaca otomatis tanpa memulai ulang (dicek lewat waktu ubah).
//! JSON yang rusak tidak membuat NOVA berhenti: dipakai bawaan, dan galatnya ditampilkan oleh `/data`.
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

pub const BERKAS: &[&str] = &["kamus_aksi.json", "cli.json", "layanan.json", "ajar.json", "putusan.json", "kasus.json", "penasaran.json", "alat.json", "bahasa.json", "contoh_niat.json", "teks.json", "pengetahuan_dasar.json", "aturan.json", "peran.json"];

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
    // berkas yang baru saja berubah dan gagal diurai mungkin sedang ditulis: jangan di-cache agar dibaca ulang
    let baru = mtime(nama).and_then(|t| t.elapsed().ok()).is_some_and(|d| d.as_secs() < 2);
    if galat.is_empty() || !baru {
        m.insert(nama.to_string(), (mtime(nama), v.clone(), galat));
    }
    v
}

fn id_entri(v: &Value) -> Option<String> {
    ["nama", "kalimat", "pola", "topik", "judul"].iter().find_map(|k| v.get(*k).and_then(|x| x.as_str())).map(|s| s.to_lowercase())
}

/// Gabungkan bawaan `a` dengan tambahan Bos `b`: objek per kunci (daftar disatukan), daftar objek per `nama`/`judul`/`kalimat`/`pola`/`topik`, lainnya ditimpa.
pub fn gabung(a: Value, b: Value) -> Value {
    match (a, b) {
        (Value::Object(mut x), Value::Object(y)) => {
            for (k, v) in y {
                let nv = match x.remove(&k) {
                    Some(old) => gabung(old, v),
                    None => v,
                };
                x.insert(k, nv);
            }
            Value::Object(x)
        }
        (Value::Array(mut x), Value::Array(y)) => {
            for v in y {
                match id_entri(&v).and_then(|k| x.iter().position(|o| id_entri(o).as_deref() == Some(k.as_str()))) {
                    Some(i) => x[i] = v,
                    None if !x.contains(&v) => x.push(v),
                    None => {}
                }
            }
            Value::Array(x)
        }
        // tambahan Bos yang kosong ([] atau {}) tidak boleh menghapus bawaan dengan bentuk lain
        (a, Value::Array(v)) if v.is_empty() => a,
        (a, Value::Object(o)) if o.is_empty() => a,
        (_, b) => b,
    }
}

type Lapis = HashMap<String, (Arc<Value>, Arc<Value>)>;
static LAPIS: Mutex<Option<Lapis>> = Mutex::new(None);

/// Data berlapis: `bawaan` (tertanam di program, ikut pembaruan) + `otak/<nama>` milik Bos yang MENAMBAH/menimpa. Berkas Bos dibuat kosong
/// bila belum ada, dan salinan bawaan ditulis ke `otak/<nama>.bawaan.json` sebagai contoh isi. Hasil digabung di-cache sampai berkas Bos berubah.
pub fn lapis(nama: &str, bawaan: &str) -> Arc<Value> {
    let kosong = if bawaan.trim_start().starts_with('[') { "[]" } else { "{}" };
    let user = json(nama, kosong);
    let mut g = LAPIS.lock().unwrap();
    let m = g.get_or_insert_with(HashMap::new);
    if let Some((u, merged)) = m.get(nama) {
        if Arc::ptr_eq(u, &user) {
            return merged.clone();
        }
    }
    let base: Value = serde_json::from_str(bawaan).unwrap_or(Value::Null);
    let _ = std::fs::write(path(&nama.replace(".json", ".bawaan.json")), serde_json::to_string_pretty(&base).unwrap_or_default());
    let merged = Arc::new(gabung(base, (*user).clone()));
    m.insert(nama.to_string(), (user, merged.clone()));
    merged
}

/// Ubah berkas lewat `f` (mulai dari isi sekarang), lalu tulis kembali rapi. Mengembalikan galat penulisan bila ada.
pub fn ubah(nama: &str, bawaan: &str, f: impl FnOnce(&mut Value)) -> Result<(), String> {
    let mut v = (*json(nama, bawaan)).clone();
    f(&mut v);
    // tulis ke berkas sementara lalu ganti (atomik): pembaca lain tidak pernah melihat berkas setengah tertulis
    let tmp = path(&format!("{nama}.tmp"));
    let r = std::fs::write(&tmp, serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?).and_then(|_| std::fs::rename(&tmp, path(nama))).map_err(|e| e.to_string());
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
    fn overlay_merges_lists_objects_and_entries_by_name() {
        let base = serde_json::json!({"stop": ["a", "b"], "slang": {"yg": "yang"}, "alat": [{"nama": "x", "d": 1}]});
        let user = serde_json::json!({"stop": ["b", "c"], "slang": {"gk": "tidak"}, "alat": [{"nama": "x", "d": 2}, {"nama": "y", "d": 3}]});
        let m = gabung(base, user);
        assert_eq!(m["stop"], serde_json::json!(["a", "b", "c"]));
        assert_eq!(m["slang"]["yg"], "yang");
        assert_eq!(m["slang"]["gk"], "tidak");
        assert_eq!(m["alat"].as_array().unwrap().len(), 2);
        assert_eq!(m["alat"][0]["d"], 2);
    }

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
