//! Jadwal sholat menurut lokasi (API Aladhan, metode Kemenag untuk Indonesia). Lokasi diatur Bos (`/sholat kota <nama>`)
//! atau dideteksi lewat IP hanya atas permintaan (`/sholat auto`). Jadwal harian disimpan lokal; pengingat 10 menit sebelum
//! dan saat waktu masuk dikirim lewat nudge asisten.
use chrono::{Local, Timelike};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicI64, Ordering};

const NAMA: [(&str, &str); 5] = [("Fajr", "Subuh"), ("Dhuhr", "Dzuhur"), ("Asr", "Ashar"), ("Maghrib", "Maghrib"), ("Isha", "Isya")];

#[derive(Serialize, Deserialize, Default, Clone)]
struct Data {
    city: String,
    country: String,
    date: String,
    /// urut: Subuh, Dzuhur, Ashar, Maghrib, Isya (HH:MM) + terbit
    times: Vec<String>,
    sunrise: String,
}

fn path() -> std::path::PathBuf {
    crate::config::data_dir().join("sholat.json")
}

fn load() -> Data {
    std::fs::read_to_string(path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save(d: &Data) {
    let _ = std::fs::write(path(), serde_json::to_string_pretty(d).unwrap_or_default());
}

fn hm(s: &str) -> String {
    s.chars().take(5).collect()
}

/// Ambil jadwal hari ini untuk kota/negara dan simpan.
pub async fn fetch(http: &reqwest::Client, city: &str, country: &str) -> Result<String, String> {
    let method = if country.to_lowercase().contains("indonesia") { 20 } else { 3 };
    let url = format!("https://api.aladhan.com/v1/timingsByCity?city={}&country={}&method={method}", urlencoding::encode(city), urlencoding::encode(country));
    let v: serde_json::Value = http.get(url).timeout(std::time::Duration::from_secs(15)).send().await.map_err(|e| e.to_string())?.json().await.map_err(|e| e.to_string())?;
    let t = &v["data"]["timings"];
    if !t.is_object() {
        return Err(format!("kota '{city}' tidak ditemukan ({})", v["data"].as_str().unwrap_or("periksa ejaan/negara")));
    }
    let times: Vec<String> = NAMA.iter().map(|(k, _)| hm(t[*k].as_str().unwrap_or("--:--"))).collect();
    let d = Data { city: city.to_string(), country: country.to_string(), date: Local::now().format("%Y-%m-%d").to_string(), times, sunrise: hm(t["Sunrise"].as_str().unwrap_or("")) };
    save(&d);
    Ok(report_of(&d))
}

fn report_of(d: &Data) -> String {
    if d.times.is_empty() {
        return "Lokasi belum diatur. Ketik: /sholat kota Jakarta (atau /sholat kota Bandung, Indonesia). /sholat auto mendeteksi dari IP (mengirim IP ke layanan ipwho.is).".into();
    }
    let now = Local::now();
    let nowm = (now.hour() * 60 + now.minute()) as i32;
    let mins: Vec<i32> = d.times.iter().map(|t| to_min(t)).collect();
    let next = mins.iter().position(|m| *m > nowm);
    let mut s = format!("🕌 Jadwal sholat {} ({}), {}\n", d.city, d.country, d.date);
    for (i, (_, n)) in NAMA.iter().enumerate() {
        s.push_str(&format!("{} {n:<8} {}\n", if Some(i) == next { "▶" } else { " " }, d.times[i]));
    }
    s.push_str(&format!("Terbit {}\n", d.sunrise));
    match next {
        Some(i) => s.push_str(&format!("Berikutnya: {} dalam {} menit.", NAMA[i].1, mins[i] - nowm)),
        None => s.push_str("Semua waktu hari ini sudah lewat; jadwal besok diambil otomatis."),
    }
    s
}

fn to_min(t: &str) -> i32 {
    let (h, m) = t.split_once(':').unwrap_or(("99", "0"));
    h.parse::<i32>().unwrap_or(99) * 60 + m.parse::<i32>().unwrap_or(0)
}

/// Perintah /sholat.
pub async fn cmd(http: &reqwest::Client, arg: &str) -> String {
    let a = arg.trim();
    let (head, rest) = a.split_once(' ').map(|(x, y)| (x, y.trim())).unwrap_or((a, ""));
    match head.to_lowercase().as_str() {
        "kota" | "lokasi" if !rest.is_empty() => {
            let (city, country) = rest.split_once(',').map(|(c, n)| (c.trim(), n.trim())).unwrap_or((rest, "Indonesia"));
            fetch(http, city, country).await.unwrap_or_else(|e| format!("Gagal: {e}"))
        }
        "auto" => {
            let v: serde_json::Value = match http.get("https://ipwho.is/").timeout(std::time::Duration::from_secs(10)).send().await {
                Ok(r) => r.json().await.unwrap_or_default(),
                Err(e) => return format!("Gagal mendeteksi lokasi: {e}"),
            };
            let (city, country) = (v["city"].as_str().unwrap_or(""), v["country"].as_str().unwrap_or(""));
            if city.is_empty() {
                return "Lokasi tidak terdeteksi. Atur manual: /sholat kota <nama>".into();
            }
            match fetch(http, city, country).await {
                Ok(r) => format!("Lokasi terdeteksi dari IP: {city}, {country}. Bila kurang tepat, atur manual dengan /sholat kota <nama>.\n{r}"),
                Err(e) => format!("Gagal: {e}"),
            }
        }
        _ => {
            let d = load();
            if !d.times.is_empty() && d.date != Local::now().format("%Y-%m-%d").to_string() {
                return fetch(http, &d.city, &d.country).await.unwrap_or_else(|e| format!("Gagal memperbarui: {e}"));
            }
            report_of(&d)
        }
    }
}

static LAST_REFRESH: AtomicI64 = AtomicI64::new(0);

/// Dipanggil asisten tiap siklus: kembalikan pengingat yang jatuh tempo (kunci de-dupe, teks). Memperbarui jadwal harian di latar.
pub fn due() -> Vec<(String, String)> {
    let d = load();
    if d.times.is_empty() {
        return vec![];
    }
    let today = Local::now().format("%Y-%m-%d").to_string();
    if d.date != today {
        let now = chrono::Utc::now().timestamp();
        if now - LAST_REFRESH.load(Ordering::Relaxed) > 1800 {
            LAST_REFRESH.store(now, Ordering::Relaxed);
            let (c, n) = (d.city.clone(), d.country.clone());
            std::thread::spawn(move || {
                if let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() {
                    let http = crate::research::client(15);
                    let _ = rt.block_on(fetch(&http, &c, &n));
                }
            });
        }
        return vec![];
    }
    let now = Local::now();
    let nowm = (now.hour() * 60 + now.minute()) as i32;
    let mut out = vec![];
    for (i, (_, n)) in NAMA.iter().enumerate() {
        let m = to_min(&d.times[i]);
        if (0..2).contains(&(nowm - m)) {
            out.push((format!("sholat{today}{n}"), format!("🕌 Waktu {n} telah masuk ({}) untuk {}. Yuk sholat; tinggalkan layar sejenak 🤲", d.times[i], d.city)));
        } else if (0..2).contains(&(nowm - (m - 10))) {
            out.push((format!("sholat{today}{n}-10"), format!("🕌 10 menit lagi {n} ({}). Siap-siap berwudhu ya.", d.times[i])));
        }
    }
    out
}
