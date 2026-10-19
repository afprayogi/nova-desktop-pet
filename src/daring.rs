//! Perkakas yang memakai internet gratis tanpa kunci: cuaca (Open-Meteo), kurs (Frankfurter), ringkasan Wikipedia, serta dua alat
//! berbasis model: penalaran langkah demi langkah dan terjemahan (Gemini bila tersedia, jika tidak Llama lokal).
use serde_json::Value;
use std::time::Duration;

fn wmo(c: i64) -> &'static str {
    match c {
        0 => "cerah",
        1 | 2 => "cerah berawan",
        3 => "berawan",
        45 | 48 => "berkabut",
        51..=57 => "gerimis",
        61..=67 => "hujan",
        71..=77 => "salju",
        80..=82 => "hujan lebat sesaat",
        95..=99 => "badai petir",
        _ => "tidak diketahui",
    }
}

async fn get(http: &reqwest::Client, url: &str) -> Result<Value, String> {
    http.get(url).timeout(Duration::from_secs(12)).send().await.map_err(|e| format!("tidak ada internet? {e}"))?.json().await.map_err(|e| format!("balasan tidak terbaca: {e}"))
}

/// Kota dari kalimat ("cuaca di bandung besok" → bandung); tanpa kota: kota jadwal sholat bila ada, jika tidak Jakarta.
fn kota_dari(text: &str) -> String {
    let l = text.to_lowercase();
    // Tanpa " di " atau awalan "cuaca " (mis. alat dipanggil perencana/jadwal dengan arg "Jakarta"): seluruh teks dianggap kota.
    let after = l.split(" di ").nth(1).or_else(|| l.strip_prefix("cuaca ")).unwrap_or(l.as_str());
    let c: Vec<&str> = after.split_whitespace().filter(|w| !["hari", "ini", "besok", "sekarang", "nanti", "lusa", "bagaimana", "gimana", "apa", "kah", "saat", "kota", "ya", "dong", "?", "cuaca", "hujan", "panas", "dingin", "cerah", "mendung", "prakiraan", "ramalan", "suhu", "akan", "apakah", "pagi", "siang", "sore", "malam"].contains(w)).collect();
    if !c.is_empty() {
        return c.join(" ");
    }
    std::fs::read_to_string(crate::config::data_dir().join("sholat.json")).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).and_then(|v| v["city"].as_str().map(String::from)).filter(|c| !c.is_empty()).unwrap_or_else(|| "Jakarta".into())
}

pub async fn cuaca(http: &reqwest::Client, text: &str) -> String {
    let kota = kota_dari(text);
    let g = match get(http, &format!("https://geocoding-api.open-meteo.com/v1/search?name={}&count=1&language=id", urlencoding::encode(&kota))).await {
        Ok(v) => v,
        Err(e) => return format!("Gagal mencari lokasi: {e}"),
    };
    let Some(p) = g["results"].get(0) else { return format!("Kota '{kota}' tidak ditemukan. Coba ejaan lain atau tambahkan negara.") };
    let (lat, lon) = (p["latitude"].as_f64().unwrap_or(0.0), p["longitude"].as_f64().unwrap_or(0.0));
    let nama = format!("{}, {}", p["name"].as_str().unwrap_or(&kota), p["country"].as_str().unwrap_or(""));
    let f = match get(http, &format!("https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}&current=temperature_2m,relative_humidity_2m,apparent_temperature,precipitation,weather_code,wind_speed_10m&daily=weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max&timezone=auto&forecast_days=3")).await {
        Ok(v) => v,
        Err(e) => return format!("Gagal mengambil prakiraan: {e}"),
    };
    let c = &f["current"];
    let mut out = format!("🌤 Cuaca {nama}: {} · {:.0}°C (terasa {:.0}°C) · lembap {}% · angin {:.0} km/jam · hujan {} mm\n", wmo(c["weather_code"].as_i64().unwrap_or(-1)), c["temperature_2m"].as_f64().unwrap_or(0.0), c["apparent_temperature"].as_f64().unwrap_or(0.0), c["relative_humidity_2m"], c["wind_speed_10m"].as_f64().unwrap_or(0.0), c["precipitation"]);
    let d = &f["daily"];
    for i in 0..3 {
        if let Some(day) = d["time"][i].as_str() {
            out.push_str(&format!("• {day}: {} · {:.0}–{:.0}°C · peluang hujan {}%\n", wmo(d["weather_code"][i].as_i64().unwrap_or(-1)), d["temperature_2m_min"][i].as_f64().unwrap_or(0.0), d["temperature_2m_max"][i].as_f64().unwrap_or(0.0), d["precipitation_probability_max"][i]));
        }
    }
    out.push_str("Sumber: Open-Meteo");
    out
}

fn kode_mata_uang(w: &str) -> Option<&'static str> {
    Some(match w.trim_matches(|c: char| !c.is_alphabetic()) {
        "usd" | "dolar" | "dollar" => "USD",
        "idr" | "rupiah" | "rp" => "IDR",
        "eur" | "euro" => "EUR",
        "jpy" | "yen" => "JPY",
        "myr" | "ringgit" => "MYR",
        "sgd" => "SGD",
        "gbp" | "pound" | "poundsterling" => "GBP",
        "aud" => "AUD",
        "cny" | "yuan" | "rmb" => "CNY",
        "sar" | "riyal" => "SAR",
        "krw" | "won" => "KRW",
        "inr" | "rupee" => "INR",
        "thb" | "baht" => "THB",
        _ => return None,
    })
}

pub async fn kurs(http: &reqwest::Client, text: &str) -> String {
    let l = text.to_lowercase().replace(',', ".");
    let toks: Vec<&str> = l.split_whitespace().collect();
    let amount: f64 = toks.iter().find_map(|t| t.trim_matches(|c: char| !c.is_ascii_digit() && c != '.').parse().ok()).unwrap_or(1.0);
    let codes: Vec<&str> = toks.iter().filter_map(|t| kode_mata_uang(t)).collect();
    let (from, to) = match codes.as_slice() {
        [a, b, ..] => (*a, *b),
        [a] if *a != "IDR" => (*a, "IDR"),
        [_] => ("USD", "IDR"),
        _ => ("USD", "IDR"),
    };
    let v = match get(http, &format!("https://api.frankfurter.app/latest?amount={amount}&from={from}&to={to}")).await {
        Ok(v) => v,
        Err(e) => return format!("Gagal mengambil kurs: {e}"),
    };
    match v["rates"][to].as_f64() {
        Some(r) => format!("💱 {amount} {from} = {:.2} {to}  (kurs referensi ECB {}; bukan kurs jual/beli bank)", r, v["date"].as_str().unwrap_or("")),
        None => format!("Kurs {from}→{to} tidak tersedia. Mata uang yang dikenali: USD, IDR, EUR, JPY, MYR, SGD, GBP, AUD, CNY, SAR, KRW, INR, THB."),
    }
}

pub async fn wiki(http: &reqwest::Client, topik: &str) -> String {
    let t = topik.trim().trim_end_matches('?');
    if t.is_empty() {
        return "Topik apa? Contoh: wikipedia transformator".into();
    }
    for lang in ["id", "en"] {
        let s = match get(http, &format!("https://{lang}.wikipedia.org/w/api.php?action=opensearch&search={}&limit=1&format=json", urlencoding::encode(t))).await {
            Ok(v) => v,
            Err(e) => return format!("Gagal mencari di Wikipedia: {e}"),
        };
        let Some(title) = s[1].get(0).and_then(|x| x.as_str()) else { continue };
        if let Ok(v) = get(http, &format!("https://{lang}.wikipedia.org/api/rest_v1/page/summary/{}", urlencoding::encode(&title.replace(' ', "_")))).await {
            if let Some(ex) = v["extract"].as_str().filter(|e| !e.is_empty()) {
                return format!("📖 {title} (Wikipedia {lang}):\n{}\n{}", crate::research::truncate(ex, 800), v["content_urls"]["desktop"]["page"].as_str().unwrap_or(""));
            }
        }
    }
    format!("Tidak ada artikel Wikipedia untuk '{t}'.")
}

async fn llm(http: &reqwest::Client, system: &str, user: &str) -> Result<String, String> {
    if crate::gemini::available() {
        if let Ok(t) = crate::gemini::generate(http, system, user, false, false).await {
            return Ok(t);
        }
    }
    crate::lokal::chat(http, &format!("{system}\n\n{user}")).await.map_err(|e| format!("model tidak tersedia: {e}"))
}

/// Penalaran langkah demi langkah (ala server MCP "sequential thinking").
pub async fn berpikir(http: &reqwest::Client, masalah: &str) -> String {
    let sys = "Kamu NOVA, penalar yang teliti. Pecahkan masalah Bos langkah demi langkah dalam Bahasa Indonesia: tulis 3-6 langkah bernomor yang singkat (setiap langkah memuat alasannya), periksa asumsi, lalu tutup dengan 'Kesimpulan:' satu-dua kalimat. Jangan mengarang angka; sebut bila data kurang.";
    match llm(http, sys, masalah).await {
        Ok(t) => format!("🧩 Penalaran:\n{t}"),
        Err(e) => e,
    }
}

pub async fn terjemah(http: &reqwest::Client, text: &str) -> String {
    // Penerjemah daring dulu (cepat, tanpa model); model bahasa hanya cadangan bila layanan tidak terjangkau.
    let (mut isi, tl) = crate::terjemah::pisah_target(text);
    for v in ["terjemahkan", "terjemah", "translate", "artikan"] {
        if isi.to_lowercase().starts_with(v) {
            isi = isi[v.len()..].trim_start_matches([':', ' ']).to_string();
            break;
        }
    }
    if let Ok((t, asal, tujuan)) = crate::terjemah::terjemahkan(http, &isi, tl).await {
        return format!("🌐 {t}
({asal} → {tujuan})");
    }
    let sys = "Kamu penerjemah profesional. Terjemahkan teks pengguna ke bahasa yang diminta (bila tidak disebut: ke Inggris jika teks berbahasa Indonesia, ke Indonesia jika bukan). Balas HANYA hasil terjemahan.";
    match llm(http, sys, text).await {
        Ok(t) => format!("🌐 {t}"),
        Err(e) => e,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn kota_dari_arg_polos_dan_kalimat() {
        assert_eq!(super::kota_dari("Jakarta"), "jakarta");
        assert_eq!(super::kota_dari("cuaca di bandung besok"), "bandung");
        assert_eq!(super::kota_dari("cuaca medan"), "medan");
        assert_eq!(super::kota_dari("apakah hujan hari ini"), super::kota_dari(""));
    }

    use super::*;
    #[test]
    fn city_and_currency_parsing() {
        assert_eq!(kota_dari("cuaca di bandung besok"), "bandung");
        assert_eq!(kode_mata_uang("dolar"), Some("USD"));
        assert_eq!(kode_mata_uang("rupiah?"), Some("IDR"));
    }
}
