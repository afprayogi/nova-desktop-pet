//! Pencarian berdasarkan MAKNA untuk pengetahuan: tiap potongan di `kb` di-embed oleh model lokal `nomic-embed-text` (Ollama, sudah
//! terpasang) dan disimpan di tabel `kb_emb`. Pertanyaan di-embed lalu dicocokkan lewat cosine, digabung dengan BM25 di `ingat::hybrid_v`.
//! Berjalan bertahap di latar; tanpa Ollama lokal pencarian kembali ke BM25 + hashing seperti biasa.

pub const AWALAN_DOK: &str = "search_document: ";
pub const AWALAN_TANYA: &str = "search_query: ";

pub fn ke_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

pub fn dari_blob(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

pub fn normal(mut v: Vec<f32>) -> Vec<f32> {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-6);
    v.iter_mut().for_each(|x| *x /= n);
    v
}

/// Embedding pertanyaan (ternormalisasi), atau None bila model lokal tidak tersedia.
pub async fn tanya(http: &reqwest::Client, q: &str) -> Option<Vec<f32>> {
    crate::lokal::ensure_server();
    let v = crate::lokal::embed(http, &[format!("{AWALAN_TANYA}{q}")]).await.ok()?.into_iter().next()?;
    (!v.is_empty()).then(|| normal(v))
}

/// Embed hingga `n` potongan yang belum punya vektor. Mengembalikan (selesai, sisa).
pub async fn isi(http: &reqwest::Client, n: usize) -> Option<(usize, i64)> {
    let rows: Vec<(i64, String)> = tokio::task::spawn_blocking(move || {
        let c = crate::teknik::open().ok()?;
        c.execute_batch("CREATE TABLE IF NOT EXISTS kb_emb(id INTEGER PRIMARY KEY, v BLOB); DELETE FROM kb_emb WHERE id NOT IN (SELECT rowid FROM kb);").ok()?;
        let mut st = c.prepare("SELECT rowid, title || '. ' || body FROM kb WHERE rowid NOT IN (SELECT id FROM kb_emb) LIMIT ?1").ok()?;
        let v: Vec<(i64, String)> = st.query_map([n as i64], |r| Ok((r.get(0)?, r.get(1)?))).ok()?.flatten().collect();
        Some(v)
    })
    .await
    .ok()??;
    if rows.is_empty() {
        return Some((0, 0));
    }
    crate::lokal::ensure_server();
    let mut selesai = 0;
    for blok in rows.chunks(16) {
        let teks: Vec<String> = blok.iter().map(|(_, t)| format!("{AWALAN_DOK}{}", t.chars().take(1200).collect::<String>())).collect();
        let vs = crate::lokal::embed(http, &teks).await.ok()?;
        let simpan: Vec<(i64, Vec<u8>)> = blok.iter().zip(vs).map(|((id, _), v)| (*id, ke_blob(&normal(v)))).collect();
        selesai += simpan.len();
        tokio::task::spawn_blocking(move || {
            if let Ok(c) = crate::teknik::open() {
                for (id, b) in simpan {
                    let _ = c.execute("INSERT OR REPLACE INTO kb_emb VALUES(?1,?2)", rusqlite::params![id, b]);
                }
            }
        })
        .await
        .ok()?;
    }
    let sisa = tokio::task::spawn_blocking(|| crate::teknik::open().ok().and_then(|c| c.query_row("SELECT COUNT(*) FROM kb WHERE rowid NOT IN (SELECT id FROM kb_emb)", [], |r| r.get(0)).ok()).unwrap_or(0)).await.unwrap_or(0);
    Some((selesai, sisa))
}

/// (potongan ber-vektor, total potongan)
pub fn status() -> (i64, i64) {
    let Ok(c) = crate::teknik::open() else { return (0, 0) };
    let _ = c.execute_batch("CREATE TABLE IF NOT EXISTS kb_emb(id INTEGER PRIMARY KEY, v BLOB);");
    (c.query_row("SELECT COUNT(*) FROM kb_emb", [], |r| r.get(0)).unwrap_or(0), c.query_row("SELECT COUNT(*) FROM kb", [], |r| r.get(0)).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blob_roundtrip_and_normalize() {
        let v = normal(vec![3.0, 4.0]);
        assert!((v[0] - 0.6).abs() < 1e-6 && (v[1] - 0.8).abs() < 1e-6);
        assert_eq!(dari_blob(&ke_blob(&v)), v);
    }
}
