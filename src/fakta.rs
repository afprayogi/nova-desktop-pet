//! Memori fakta jangka panjang tentang Bos, terinspirasi mem0 (ekstraksi fakta → simpan/perbarui, bukan menyimpan seluruh
//! percakapan): kategori preferensi, detail pribadi, pekerjaan/proyek, dan catatan. Ekstraksi berbasis pola bahasa Indonesia
//! (tanpa panggilan model, hemat token). Kunci yang sama diperbarui, bukan digandakan. Fakta disisipkan ke konteks Gemini/Llama.
use rusqlite::Connection;

fn db() -> Option<Connection> {
    let c = crate::assistant::db().ok()?;
    c.execute_batch("CREATE TABLE IF NOT EXISTS fakta(k TEXT PRIMARY KEY, kategori TEXT, isi TEXT, ts INT);").ok()?;
    Some(c)
}

fn clean(s: &str) -> String {
    s.trim().trim_matches(|c: char| c == '.' || c == ',' || c == '!' || c == '?' || c == '"').trim().chars().take(80).collect()
}

/// (kunci, kategori, isi) yang ditemukan dalam satu kalimat/klausa.
fn extract_clause(c: &str) -> Vec<(String, &'static str, String)> {
    let l = c.trim();
    let low = l.to_lowercase();
    let mut out = vec![];
    let after = |pre: &str| -> Option<String> { low.find(pre).map(|i| clean(&l[i + pre.len()..])).filter(|x| !x.is_empty() && x.split_whitespace().count() <= 8) };
    if let Some(n) = after("nama saya ").or_else(|| after("namaku ")).or_else(|| after("nama aku ")) {
        out.push(("nama".into(), "pribadi", n));
    }
    if let Some(n) = after("panggil saya ").or_else(|| after("panggil aku ")) {
        out.push(("panggilan".into(), "pribadi", n));
    }
    if let Some(x) = after("saya tidak suka ").or_else(|| after("aku tidak suka ")).or_else(|| after("saya benci ")) {
        out.push((format!("tidak_suka:{}", x.to_lowercase()), "preferensi", format!("tidak suka {x}")));
    } else if let Some(x) = after("saya suka ").or_else(|| after("aku suka ")).or_else(|| after("saya senang ")).or_else(|| after("saya hobi ")) {
        out.push((format!("suka:{}", x.to_lowercase()), "preferensi", format!("suka {x}")));
    }
    if let Some(x) = after("saya kerja di ").or_else(|| after("saya bekerja di ")).or_else(|| after("aku kerja di ")) {
        out.push(("kerja_di".into(), "pekerjaan", format!("bekerja di {x}")));
    }
    if let Some(x) = after("saya bekerja sebagai ").or_else(|| after("saya seorang ")).or_else(|| after("pekerjaan saya ")) {
        out.push(("profesi".into(), "pekerjaan", x));
    }
    if let Some(x) = after("saya tinggal di ").or_else(|| after("aku tinggal di ")) {
        out.push(("tinggal".into(), "pribadi", format!("tinggal di {x}")));
    }
    if let Some(x) = after("saya sedang belajar ").or_else(|| after("saya lagi belajar ")).or_else(|| after("saya sedang mengerjakan ")).or_else(|| after("saya lagi ngerjain ")) {
        out.push((format!("proyek:{}", x.to_lowercase()), "proyek", format!("sedang mengerjakan/belajar {x}")));
    }
    if let Some(x) = after("ingat bahwa ").or_else(|| after("tolong ingat ")).or_else(|| after("ingat, ")) {
        out.push((format!("catatan:{}", x.to_lowercase()), "catatan", x));
    }
    out
}

/// Ekstrak dan simpan fakta dari ucapan Bos. Mengembalikan fakta yang disimpan/diperbarui.
pub fn extract(text: &str) -> Vec<String> {
    if text.starts_with('/') || text.len() < 8 {
        return vec![];
    }
    let Some(c) = db() else { return vec![] };
    let mut saved = vec![];
    for clause in text.split(|ch| ch == ',' || ch == ';').flat_map(|p| p.split(" dan ")) {
        for (k, cat, isi) in extract_clause(clause) {
            let old: Option<String> = c.query_row("SELECT isi FROM fakta WHERE k=?1", [&k], |r| r.get(0)).ok();
            if c.execute("INSERT OR REPLACE INTO fakta VALUES(?1,?2,?3,strftime('%s','now'))", rusqlite::params![k, cat, isi]).is_ok() && old.as_deref() != Some(&isi) {
                saved.push(if k == "nama" { format!("nama: {isi}") } else { isi });
            }
        }
    }
    saved
}

/// Ringkasan fakta untuk konteks prompt (maks ±12 butir, terbaru dulu).
pub fn ringkas() -> String {
    let Some(c) = db() else { return String::new() };
    let Ok(mut st) = c.prepare("SELECT k, isi FROM fakta ORDER BY ts DESC LIMIT 12") else { return String::new() };
    let rows: Vec<(String, String)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).map(|r| r.flatten().collect()).unwrap_or_default();
    rows.into_iter().map(|(k, i)| if k == "nama" || k == "panggilan" { format!("{k} Bos: {i}") } else { i }).collect::<Vec<_>>().join("; ")
}

pub fn daftar() -> String {
    let Some(c) = db() else { return "Memori fakta tidak tersedia.".into() };
    let Ok(mut st) = c.prepare("SELECT kategori, isi FROM fakta ORDER BY kategori, ts DESC") else { return String::new() };
    let rows: Vec<(String, String)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).map(|r| r.flatten().collect()).unwrap_or_default();
    if rows.is_empty() {
        return "Belum ada fakta tentang Bos yang kuingat. Katakan misalnya: 'nama saya Fauzan', 'saya suka PLC', 'saya kerja di ...', atau 'ingat bahwa ...'.".into();
    }
    let mut s = format!("🧠 {} fakta yang kuingat tentang Bos:\n", rows.len());
    let mut cur = String::new();
    for (k, i) in rows {
        if k != cur {
            s.push_str(&format!("\n[{k}]\n"));
            cur = k;
        }
        s.push_str(&format!("• {i}\n"));
    }
    s.push_str("\nHapus: /lupa <kata kunci> · semua: /lupa semua");
    s
}

pub fn lupa(arg: &str) -> String {
    let Some(c) = db() else { return "Memori fakta tidak tersedia.".into() };
    let a = arg.trim().to_lowercase();
    if a.is_empty() {
        return "Pakai: /lupa <kata kunci> atau /lupa semua".into();
    }
    let n = if a == "semua" { c.execute("DELETE FROM fakta", []).unwrap_or(0) } else { c.execute("DELETE FROM fakta WHERE lower(isi) LIKE ?1 OR lower(k) LIKE ?1", [format!("%{a}%")]).unwrap_or(0) };
    format!("🗑 {n} fakta dilupakan.")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extracts_indonesian_facts() {
        let v = extract_clause("nama saya Fauzan");
        assert_eq!(v[0], ("nama".to_string(), "pribadi", "Fauzan".to_string()));
        let v = extract_clause("saya suka PLC");
        assert_eq!(v[0].2, "suka PLC");
        let v = extract_clause("saya tidak suka rapat panjang");
        assert!(v[0].0.starts_with("tidak_suka:"));
        assert!(extract_clause("cuaca hari ini bagaimana").is_empty());
    }
}
