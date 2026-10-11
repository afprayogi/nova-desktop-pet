//! Cari gambar di internet lewat Wikimedia Commons (API publik, tanpa kunci, berlisensi bebas). Hasil: baris `IMG <url>`
//! yang ditampilkan sebagai thumbnail di antarmuka web, plus judul dan tautan sumber.
use serde_json::Value;

pub async fn cari(http: &reqwest::Client, query: &str) -> String {
    let q = query.trim();
    if q.is_empty() {
        return "Gambar apa yang dicari? Contoh: carikan gambar relay 8 pin".into();
    }
    let url = format!("https://commons.wikimedia.org/w/api.php?action=query&generator=search&gsrsearch={}&gsrnamespace=6&gsrlimit=6&prop=imageinfo&iiprop=url&iiurlwidth=400&format=json", urlencoding::encode(q));
    let v: Value = match http.get(url).timeout(std::time::Duration::from_secs(12)).send().await {
        Ok(r) => r.json().await.unwrap_or(Value::Null),
        Err(e) => return format!("Gagal mencari gambar (tidak ada internet?): {e}"),
    };
    let mut pages: Vec<&Value> = v["query"]["pages"].as_object().map(|o| o.values().collect()).unwrap_or_default();
    pages.sort_by_key(|p| p["index"].as_i64().unwrap_or(99));
    let mut out = String::new();
    for p in pages.iter().filter(|p| {
        let t = p["title"].as_str().unwrap_or("").to_lowercase();
        t.ends_with(".jpg") || t.ends_with(".jpeg") || t.ends_with(".png") || t.ends_with(".webp") || t.ends_with(".gif")
    }).take(4) {
        let info = &p["imageinfo"][0];
        if let (Some(th), Some(page)) = (info["thumburl"].as_str(), info["descriptionurl"].as_str()) {
            out.push_str(&format!("IMG {th}\n{}\n{page}\n", p["title"].as_str().unwrap_or("").trim_start_matches("File:")));
        }
    }
    if out.is_empty() {
        return format!("Tidak menemukan gambar bebas lisensi untuk '{q}'. Coba kata kunci bahasa Inggris, atau cari manual: https://www.google.com/search?tbm=isch&q={}", urlencoding::encode(q));
    }
    format!("🖼 Gambar untuk '{q}' (Wikimedia Commons, lisensi bebas):\n{out}Lebih banyak: https://www.google.com/search?tbm=isch&q={}", urlencoding::encode(q))
}
