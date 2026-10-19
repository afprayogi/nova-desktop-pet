//! Pencarian web tanpa key: DuckDuckGo/SearXNG -> baca 3 halaman teratas (hanya situs publik, maks 1 MB) -> jawaban bersumber.
//! Dipakai saat Gemini+Google Search tidak ada/kuotanya habis, supaya NOVA tetap benar-benar mencari, bukan menjawab dari ingatan.
//! Dengan model (Gemini/lokal) jawabannya dirangkai; tanpa model, kalimat paling relevan dari tiap halaman ditampilkan apa adanya.
use crate::research::{self, Hit};
use std::time::Duration;

const STOP: &[&str] = &["yang", "dan", "di", "ke", "dari", "untuk", "adalah", "apa", "itu", "ini", "cara", "bagaimana", "siapa", "kapan", "dengan", "pada", "the", "of", "to", "in", "a", "is", "for"];

fn istilah(q: &str) -> Vec<String> {
    q.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| w.len() > 2 && !STOP.contains(w)).map(String::from).collect()
}

/// Kalimat dengan kata kunci terbanyak (urutan asli dipertahankan).
fn kalimat_relevan(teks: &str, ist: &[String], n: usize) -> Vec<String> {
    let mut sk: Vec<(usize, usize, &str)> = teks
        .split(|c| matches!(c, '.' | '!' | '?'))
        .map(str::trim)
        .enumerate()
        .filter(|(_, s)| (40..=300).contains(&s.chars().count()))
        .map(|(i, s)| {
            let l = s.to_lowercase();
            (ist.iter().filter(|w| l.contains(w.as_str())).count(), i, s)
        })
        .filter(|(c, _, _)| *c > 0)
        .collect();
    sk.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    sk.truncate(n);
    sk.sort_by_key(|x| x.1);
    sk.into_iter().map(|x| format!("{}.", x.2)).collect()
}

/// Kueri mesin pencari: buang kata tanya/penghubung ("siapa ..." sering dijawab dengan arti kata, bukan isinya).
fn kueri(q: &str) -> String {
    let l = q.to_lowercase();
    let mut kata: Vec<String> = Vec::new();
    for w in l.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()) {
        if !crate::lex::ada("stop_tanya", w) && !STOP.contains(&w) && !kata.iter().any(|k| k == w) {
            kata.push(w.to_string());
        }
    }
    kata.truncate(8);
    if kata.is_empty() { q.trim().to_string() } else { kata.join(" ") }
}

pub async fn jawab(http: &reqwest::Client, q: &str) -> Option<String> {
    // Hemat: pertanyaan yang sama dalam 6 jam dijawab dari memori (tanpa mencari/panggil model lagi).
    if let Some(t) = crate::gemini::cache_get("web", q, 6 * 3600) {
        return Some(format!("💾 {t}"));
    }
    let r = cari_dan_rangkum(http, q).await?;
    if !r.contains("sumber") || !r.starts_with("🌐 Maaf") {
        crate::gemini::cache_put("web", q, &r);
        crate::teknik::simpan_jawaban("web", q, &r);
    }
    Some(r)
}

async fn cari_dan_rangkum(http: &reqwest::Client, q: &str) -> Option<String> {
    let job = crate::progres::mulai(&format!("Mencari: {}", research::truncate(q, 40)), "mencari di web");
    let r = cari_lapor(http, q, job).await;
    match &r {
        Some(_) => crate::progres::selesai(job, "jawaban siap"),
        None => crate::progres::gagal(job, "tidak ada hasil"),
    }
    r
}

async fn cari_lapor(http: &reqwest::Client, q: &str, job: u64) -> Option<String> {
    let searx = crate::config::load().searxng_url;
    let cari = kueri(q);
    let hits: Vec<Hit> = research::search(http, &searx, &cari).await.ok()?.into_iter().take(4).collect();
    if hits.is_empty() {
        return None;
    }
    let ist = istilah(q);
    crate::progres::set(job, 30, "membaca halaman teratas");
    let halaman = futures::future::join_all(hits.iter().take(3).map(|h| async move {
        let r = tokio::time::timeout(Duration::from_secs(10), crate::alat::ambil_publik(&h.url)).await.ok()?.ok()?;
        Some(crate::peramban::teks(&r))
    }))
    .await;
    let sumber: Vec<(&Hit, String)> = hits
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let mut k = halaman.get(i).and_then(|p| p.as_deref()).map(|t| kalimat_relevan(t, &ist, 2)).unwrap_or_default();
            if k.is_empty() {
                k.push(h.snippet.clone());
            }
            (h, research::truncate(&k.join(" "), 500))
        })
        .collect();
    crate::progres::set(job, 80, "merangkum jawaban");
    let konteks = sumber.iter().enumerate().map(|(i, (h, k))| format!("[{}] {} ({}): {k}", i + 1, h.title, h.url)).collect::<Vec<_>>().join("\n");
    let sys = "Kamu NOVA, asisten yang hangat (panggil \"Bos\"). Jawab dalam Bahasa Indonesia, ringkas (maksimal 5 kalimat), HANYA berdasarkan SUMBER berikut dan sebut nomor sumbernya seperti [1]. Bila sumber tidak cukup, katakan terus terang.";
    let prompt = format!("Pertanyaan: {q}\n\nSUMBER:\n{konteks}");
    let jawaban = if crate::gemini::available() {
        crate::gemini::generate(http, sys, &prompt, false, false).await.ok()
    } else if crate::lokal::has(http, crate::lokal::CHAT_MODEL).await {
        crate::lokal::chat(http, &format!("{sys}\n\n{prompt}")).await.ok()
    } else {
        None
    };
    let isi = jawaban.unwrap_or_else(|| sumber.iter().enumerate().map(|(i, (_, k))| format!("[{}] {k}", i + 1)).collect::<Vec<_>>().join("\n"));
    let daftar = sumber.iter().enumerate().map(|(i, (h, _))| format!("[{}] {} — {}", i + 1, research::truncate(&h.title, 60), h.url)).collect::<Vec<_>>().join("\n");
    Some(format!("🌐 {isi}\n\nSumber:\n{daftar}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kueri_membuang_kata_tanya() {
        let k = kueri("siapa pemenang piala dunia terakhir");
        assert!(!k.contains("siapa") && k.contains("piala") && k.contains("terakhir"));
    }

    #[test]
    fn memilih_kalimat_yang_memuat_kata_kunci() {
        let ist = istilah("drop tegangan kabel");
        let t = "Cuaca hari ini cerah di seluruh kota dan sekitarnya sepanjang hari. Drop tegangan pada kabel dihitung dari panjang dan luas penampang penghantar. Resep nasi goreng butuh bawang putih dan kecap manis secukupnya saja.";
        let k = kalimat_relevan(t, &ist, 1);
        assert_eq!(k.len(), 1);
        assert!(k[0].starts_with("Drop tegangan"));
    }
}
