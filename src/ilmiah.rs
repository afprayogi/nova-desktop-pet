//! Pencarian jurnal/makalah ilmiah lewat OpenAlex (API publik, gratis, tanpa kunci). Mengurai rentang tahun dari kalimat
//! ("tahun 2026 kurang 5 tahun" → 2021–2026) dan membuang kata perintah dari kata kunci.
use serde_json::Value;

const SKIP: &[&str] = &["coba", "carikan", "cari", "cariin", "jurnal", "jurnalnya", "terbaru", "tahun", "kurang", "paper", "makalah", "artikel", "ilmiah", "publikasi", "tentang", "yang", "di", "dan", "dari", "sampai", "hingga", "untuk", "saya", "tolong", "nova", "halo", "tentang", "internet", "online", "terkait"];

/// (kata kunci, tahun awal, tahun akhir)
pub fn parse(text: &str) -> (String, i32, i32) {
    let now_year: i32 = chrono::Datelike::year(&chrono::Local::now());
    let l = text.to_lowercase();
    let words: Vec<&str> = l.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let years: Vec<i32> = words.iter().filter_map(|w| w.parse::<i32>().ok()).filter(|y| (1990..=2100).contains(y)).collect();
    let kurang: Option<i32> = words.iter().position(|w| *w == "kurang").and_then(|i| words.get(i + 1)).and_then(|w| w.parse().ok()).filter(|n| (1..=50).contains(n));
    let end = years.iter().copied().max().unwrap_or(now_year).min(now_year + 1);
    let start = match (kurang, years.len()) {
        (Some(n), _) => end - n,
        (None, n) if n >= 2 => years.iter().copied().min().unwrap_or(end - 5),
        _ => end - 5,
    };
    let q: Vec<&str> = words.iter().copied().filter(|w| !SKIP.contains(w) && w.parse::<i32>().is_err()).collect();
    (q.join(" "), start, end)
}

pub async fn cari(http: &reqwest::Client, text: &str) -> String {
    let (q, y0, y1) = parse(text);
    if q.is_empty() {
        return "Jurnal tentang apa? Contoh: carikan jurnal semantic search tahun 2026 kurang 5 tahun".into();
    }
    let url = format!(
        "https://api.openalex.org/works?search={}&filter=from_publication_date:{y0}-01-01,to_publication_date:{y1}-12-31,type:article&per-page=6&select=title,publication_year,doi,cited_by_count,primary_location,authorships",
        urlencoding::encode(&q)
    );
    let v: Value = match http.get(url).timeout(std::time::Duration::from_secs(15)).send().await {
        Ok(r) => r.json().await.unwrap_or(Value::Null),
        Err(e) => return format!("Gagal mencari jurnal (tidak ada internet?): {e}"),
    };
    let Some(items) = v["results"].as_array().filter(|a| !a.is_empty()) else {
        return format!("Tidak ada artikel untuk '{q}' ({y0}–{y1}). Coba kata kunci bahasa Inggris, atau cari di https://scholar.google.com/scholar?q={}", urlencoding::encode(&q));
    };
    let mut out = format!("📚 Jurnal '{q}' ({y0}–{y1}), sumber OpenAlex:\n");
    for (i, w) in items.iter().enumerate() {
        let title = w["title"].as_str().unwrap_or("(tanpa judul)");
        let venue = w["primary_location"]["source"]["display_name"].as_str().unwrap_or("-");
        let first = w["authorships"][0]["author"]["display_name"].as_str().unwrap_or("");
        out.push_str(&format!("{}. {} ({}) — {}{} · dikutip {}×\n   {}\n", i + 1, title, w["publication_year"], venue, if first.is_empty() { String::new() } else { format!(" · {first} dkk.") }, w["cited_by_count"], w["doi"].as_str().unwrap_or("")));
    }
    out.push_str(&format!("Lebih banyak: https://scholar.google.com/scholar?q={}&as_ylo={y0}", urlencoding::encode(&q)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_year_ranges() {
        let (q, a, b) = parse("coba carikan jurnal terbaru semantic di tahun 2026 kurang 5 tahun");
        assert_eq!((q.as_str(), a, b), ("semantic", 2021, 2026));
        let (q, a, b) = parse("cari paper deep learning 2019 sampai 2023");
        assert_eq!((q.as_str(), a, b), ("deep learning", 2019, 2023));
    }
}
