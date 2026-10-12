//! Penerjemah tanpa model lokal (cepat, hampir tanpa RAM): layanan terjemahan daring publik tanpa kunci — Google Translate endpoint
//! `gtx` (utama, deteksi bahasa otomatis) dengan cadangan MyMemory. Teks dipecah per kalimat (≤1500 karakter per permintaan) dan
//! hasilnya di-cache di RAM. CATATAN PRIVASI: teks yang diterjemahkan dikirim ke layanan itu; isi berkas/email tidak pernah dikirim otomatis.
use anyhow::{anyhow, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;

const BAHASA: &[(&str, &str)] = &[
    ("inggris", "en"), ("english", "en"), ("indonesia", "id"), ("jepang", "ja"), ("korea", "ko"), ("mandarin", "zh-CN"), ("cina", "zh-CN"),
    ("arab", "ar"), ("jerman", "de"), ("prancis", "fr"), ("perancis", "fr"), ("spanyol", "es"), ("belanda", "nl"), ("rusia", "ru"),
    ("melayu", "ms"), ("jawa", "jv"), ("sunda", "su"), ("italia", "it"), ("portugis", "pt"), ("thailand", "th"), ("turki", "tr"), ("hindi", "hi"),
];

static CACHE: Mutex<Option<HashMap<(String, String), (String, String)>>> = Mutex::new(None);

/// Pisahkan permintaan bahasa tujuan ("... ke inggris", "dalam bahasa jepang") dari teks. Mengembalikan (teks, kode bahasa).
pub fn pisah_target(arg: &str) -> (String, Option<&'static str>) {
    let kata: Vec<&str> = arg.split_whitespace().collect();
    let cari = |i: usize| BAHASA.iter().find(|(n, _)| kata.get(i).is_some_and(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).eq_ignore_ascii_case(n))).map(|(_, k)| *k);
    for i in 0..kata.len() {
        let pj = if kata[i].eq_ignore_ascii_case("ke") && kata.get(i + 1).is_some_and(|w| w.eq_ignore_ascii_case("bahasa")) { 3 } else if kata[i].eq_ignore_ascii_case("ke") { 2 } else if kata[i].eq_ignore_ascii_case("dalam") && kata.get(i + 1).is_some_and(|w| w.eq_ignore_ascii_case("bahasa")) { 3 } else { 0 };
        if pj == 0 {
            continue;
        }
        if let Some(k) = cari(i + pj - 1) {
            let sisa: Vec<&str> = kata[..i].iter().chain(kata[i + pj..].iter()).copied().collect();
            let teks = sisa.join(" ");
            return (teks, Some(k));
        }
    }
    (arg.trim().to_string(), None)
}

/// Segmen dan bahasa asal dari balasan gtx: `[[["terjemah","asli",..],..],null,"en",..]`.
pub fn urai_gtx(v: &Value) -> Option<(String, String)> {
    let t: String = v.get(0)?.as_array()?.iter().filter_map(|s| s.get(0).and_then(|x| x.as_str())).collect();
    (!t.is_empty()).then(|| (t, v.get(2).and_then(|x| x.as_str()).unwrap_or("").to_string()))
}

async fn gtx(http: &reqwest::Client, q: &str, tl: &str) -> Result<(String, String)> {
    let v: Value = http
        .post("https://translate.googleapis.com/translate_a/single")
        .timeout(std::time::Duration::from_secs(15))
        .form(&[("client", "gtx"), ("sl", "auto"), ("tl", tl), ("dt", "t"), ("q", q)])
        .send()
        .await?
        .json()
        .await?;
    urai_gtx(&v).ok_or_else(|| anyhow!("balasan gtx kosong"))
}

async fn mymemory(http: &reqwest::Client, q: &str, tl: &str) -> Result<String> {
    let q: String = q.chars().take(450).collect(); // batas 500 byte per permintaan
    let url = format!("https://api.mymemory.translated.net/get?q={}&langpair=Autodetect|{tl}", urlencoding::encode(&q));
    let v: Value = http.get(&url).timeout(std::time::Duration::from_secs(15)).send().await?.json().await?;
    v["responseData"]["translatedText"].as_str().filter(|t| !t.is_empty()).map(String::from).ok_or_else(|| anyhow!("balasan MyMemory kosong"))
}

async fn satu(http: &reqwest::Client, q: &str, tl: &str) -> Result<(String, String)> {
    let kunci = (q.to_string(), tl.to_string());
    if let Some(h) = CACHE.lock().ok().and_then(|c| c.as_ref().and_then(|m| m.get(&kunci).cloned())) {
        return Ok(h);
    }
    let r = match gtx(http, q, tl).await {
        Ok(r) => r,
        Err(_) => (mymemory(http, q, tl).await?, "?".into()),
    };
    if let Ok(mut c) = CACHE.lock() {
        let m = c.get_or_insert_with(HashMap::new);
        if m.len() > 500 {
            m.clear();
        }
        m.insert(kunci, r.clone());
    }
    Ok(r)
}

/// Terjemahkan `teks`. Tanpa `target`: ke Indonesia, atau ke Inggris bila teksnya sudah Indonesia.
/// Mengembalikan (hasil, kode bahasa asal, kode bahasa tujuan).
pub async fn terjemahkan(http: &reqwest::Client, teks: &str, target: Option<&str>) -> Result<(String, String, String)> {
    let teks = teks.trim();
    if teks.is_empty() {
        return Err(anyhow!("teks kosong"));
    }
    let potong: Vec<&str> = text_splitter::TextSplitter::new(200..1500).chunks(teks).take(12).collect();
    let mut tl = target.unwrap_or("id").to_string();
    let mut hasil = String::new();
    let mut asal = String::new();
    for (i, c) in potong.iter().enumerate() {
        let (mut t, mut s) = satu(http, c, &tl).await?;
        if i == 0 && target.is_none() && s == "id" {
            // Teks sudah Indonesia: balik ke Inggris.
            tl = "en".into();
            (t, s) = satu(http, c, &tl).await?;
        }
        if i == 0 {
            asal = s;
        }
        hasil.push_str(&t);
        hasil.push('\n');
    }
    Ok((hasil.trim().to_string(), asal, tl))
}

/// Tebakan cepat: teks berbahasa Inggris (kata fungsi Inggris jauh lebih banyak daripada Indonesia).
pub fn kemungkinan_inggris(teks: &str) -> bool {
    const EN: &[&str] = &["the", "of", "and", "is", "are", "to", "in", "that", "with", "for", "this", "which", "be", "as", "by", "from", "or", "an", "it"];
    const ID: &[&str] = &["yang", "dan", "di", "untuk", "dengan", "ini", "itu", "adalah", "pada", "dari", "atau", "dalam", "akan", "juga"];
    let kata: Vec<String> = teks.to_lowercase().split(|c: char| !c.is_alphabetic()).filter(|w| !w.is_empty()).map(String::from).collect();
    if kata.len() < 12 {
        return false;
    }
    let (en, id) = (kata.iter().filter(|w| EN.contains(&w.as_str())).count(), kata.iter().filter(|w| ID.contains(&w.as_str())).count());
    en * 100 / kata.len() >= 8 && en > id * 2
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn target_and_gtx_parsing() {
        assert_eq!(pisah_target("good morning ke jepang"), ("good morning".to_string(), Some("ja")));
        assert_eq!(pisah_target("terjemahkan hello world ke bahasa Inggris"), ("terjemahkan hello world".to_string(), Some("en")));
        assert_eq!(pisah_target("hello world").1, None);
        let v: Value = serde_json::from_str(r#"[[["Halo dunia","Hello world",null,null,10],[" lagi","again",null,null,1]],null,"en"]"#).unwrap();
        assert_eq!(urai_gtx(&v), Some(("Halo dunia lagi".to_string(), "en".to_string())));
    }
    #[test]
    fn detects_english() {
        assert!(kemungkinan_inggris("The motor starter is a device that is used to start and protect the motor from overload in an industrial plant."));
        assert!(!kemungkinan_inggris("Starter motor adalah perangkat yang dipakai untuk menjalankan dan melindungi motor dari beban lebih pada pabrik."));
        assert!(!kemungkinan_inggris("pendek saja"));
    }
}
