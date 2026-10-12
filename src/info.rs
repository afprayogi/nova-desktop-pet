//! Folder `otak/info/`: berkas `.md`, `.txt`, `.json`, `.pdf` yang Bos taruh di sana otomatis masuk ke pengetahuan (sumber `info:<nama>`),
//! diperbarui saat berkas berubah, dan dihapus dari otak saat berkasnya dihapus. Pemindaian tiap menit di daemon atau `/info`.
use std::path::PathBuf;

pub fn folder() -> PathBuf {
    let d = crate::config::otak_dir().join("info");
    let _ = std::fs::create_dir_all(&d);
    d
}

fn teks_json(v: &serde_json::Value, out: &mut String) {
    match v {
        serde_json::Value::String(s) => {
            out.push_str(s);
            out.push('\n');
        }
        serde_json::Value::Array(a) => a.iter().for_each(|x| teks_json(x, out)),
        serde_json::Value::Object(o) => o.iter().for_each(|(k, x)| {
            if x.is_string() {
                out.push_str(&format!("{k}: "));
            }
            teks_json(x, out)
        }),
        _ => {}
    }
}

/// Isi berkas sebagai teks biasa.
pub fn baca(p: &std::path::Path) -> Result<String, String> {
    let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("").to_lowercase();
    match ext.as_str() {
        "json" => {
            let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).map_err(|e| e.to_string())?).map_err(|e| format!("JSON rusak: {e}"))?;
            let mut s = String::new();
            teks_json(&v, &mut s);
            Ok(s)
        }
        "md" | "txt" | "pdf" => crate::alat::read_doc_max(&p.to_string_lossy(), 3_000_000),
        _ => Err("jenis berkas tidak didukung".into()),
    }
}

/// Sinkronkan folder dengan otak. Mengembalikan ringkasan (kosong bila tidak ada perubahan).
pub fn sinkron() -> String {
    let Ok(c) = crate::teknik::open() else { return String::new() };
    let _ = c.execute_batch("CREATE TABLE IF NOT EXISTS info_seen(nama TEXT PRIMARY KEY, sig TEXT);");
    let (mut baru, mut gagal, mut ada) = (0, vec![], std::collections::HashSet::new());
    if let Ok(rd) = std::fs::read_dir(folder()) {
        for e in rd.flatten() {
            let p = e.path();
            let Some(nama) = p.file_name().map(|n| n.to_string_lossy().into_owned()) else { continue };
            if !p.is_file() || !["md", "txt", "json", "pdf"].contains(&p.extension().and_then(|x| x.to_str()).unwrap_or("").to_lowercase().as_str()) {
                continue;
            }
            ada.insert(nama.clone());
            let m = e.metadata().ok();
            let sig = format!("{}-{}", m.as_ref().map(|m| m.len()).unwrap_or(0), m.and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0));
            if c.query_row("SELECT sig FROM info_seen WHERE nama=?1", [&nama], |r| r.get::<_, String>(0)).ok().as_deref() == Some(sig.as_str()) {
                continue;
            }
            match baca(&p) {
                Ok(t) => {
                    crate::teknik::ingest_max(&format!("info:{nama}"), &t, 5000);
                    let _ = c.execute("INSERT OR REPLACE INTO info_seen VALUES(?1,?2)", (&nama, &sig));
                    baru += 1;
                }
                Err(e) => gagal.push(format!("{nama} ({e})")),
            }
        }
    }
    // berkas yang sudah dihapus dari folder dibuang dari otak
    let lama: Vec<String> = c.prepare("SELECT nama FROM info_seen").and_then(|mut s| s.query_map([], |r| r.get::<_, String>(0)).map(|r| r.flatten().collect())).unwrap_or_default();
    let mut hapus = 0;
    for n in lama.into_iter().filter(|n| !ada.contains(n)) {
        let _ = c.execute("DELETE FROM kb WHERE src=?1", [format!("info:{n}")]);
        let _ = c.execute("DELETE FROM info_seen WHERE nama=?1", [&n]);
        hapus += 1;
    }
    if baru + hapus > 0 {
        crate::ingat::clear_cache();
    }
    if baru == 0 && hapus == 0 && gagal.is_empty() {
        return String::new();
    }
    format!("📂 Folder info: {baru} berkas dipelajari, {hapus} dihapus dari otak{}.", if gagal.is_empty() { String::new() } else { format!("; gagal: {}", gagal.join(", ")) })
}

pub fn daftar() -> String {
    let n = std::fs::read_dir(folder()).map(|r| r.flatten().filter(|e| e.path().is_file()).count()).unwrap_or(0);
    format!("Folder info: {} ({n} berkas). Taruh .md/.txt/.json/.pdf di sana; otomatis masuk otak tiap menit. /info sinkron untuk sekarang.", folder().display())
}

#[cfg(test)]
mod tests {
    #[test]
    fn json_becomes_text() {
        let v: serde_json::Value = serde_json::from_str(r#"{"judul":"Relai","isi":["a","b"],"n":3}"#).unwrap();
        let mut s = String::new();
        super::teks_json(&v, &mut s);
        assert!(s.contains("judul: Relai") && s.contains('a') && !s.contains('3'));
    }
}
