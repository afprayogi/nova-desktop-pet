//! Harga & pemantau: kuotasi saham/kurs/emas/kripto lewat Yahoo Finance (endpoint publik tanpa kunci), grafik 30 hari yang
//! dikirim sebagai baris `CHART {json}` (digambar di obrolan web dan pop-up), pemantau ambang harga, dan pengingat cuaca
//! (hujan/panas) lewat Open-Meteo. Peringatan masuk notifikasi, tanpa mengganggu obrolan.
use crate::engine::Tx;
use serde_json::Value;
use std::time::Duration;

fn db() -> Option<rusqlite::Connection> {
    let c = crate::assistant::db().ok()?;
    c.execute_batch("CREATE TABLE IF NOT EXISTS pantau(id INTEGER PRIMARY KEY, sym TEXT, arah TEXT, batas REAL, alert_ts INT DEFAULT 0); CREATE TABLE IF NOT EXISTS cuaca_pantau(kota TEXT PRIMARY KEY, hari TEXT DEFAULT '', hujan_ts INT DEFAULT 0);").ok()?;
    Some(c)
}

/// Nama bebas → simbol Yahoo.
pub fn simbol(teks: &str) -> Option<(String, String)> {
    let l = teks.to_lowercase();
    let known: &[(&[&str], &str, &str)] = &[
        (&["usd idr", "usdidr", "dolar", "dollar", "kurs"], "IDR=X", "USD/IDR"),
        (&["eur idr", "euro"], "EURIDR=X", "EUR/IDR"),
        (&["ihsg", "indeks saham"], "^JKSE", "IHSG"),
        (&["emas", "gold"], "GC=F", "Emas (USD/oz)"),
        (&["bitcoin", "btc"], "BTC-USD", "Bitcoin (USD)"),
        (&["ethereum", "eth"], "ETH-USD", "Ethereum (USD)"),
        (&["minyak", "oil"], "CL=F", "Minyak WTI (USD)"),
        (&["nasdaq"], "^IXIC", "NASDAQ"),
        (&["s&p", "sp500"], "^GSPC", "S&P 500"),
    ];
    for (keys, sym, nama) in known {
        if keys.iter().any(|k| l.contains(k)) {
            return Some((sym.to_string(), nama.to_string()));
        }
    }
    // kode saham: 4 huruf (BBCA) → bursa Indonesia; selain itu apa adanya (AAPL, TSLA, BBCA.JK)
    let tok = teks.split_whitespace().rev().find(|w| w.len() >= 2 && w.len() <= 12 && w.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '^' || c == '='))?;
    let up = tok.to_uppercase();
    let us = ["AAPL", "TSLA", "MSFT", "GOOG", "AMZN", "NVDA", "META", "NFLX", "AMD", "INTC", "ORCL", "IBM", "UBER", "BABA", "PYPL", "SPOT", "COIN", "NIO", "SHOP", "SNAP"];
    let sym = if up.len() == 4 && up.chars().all(|c| c.is_ascii_alphabetic()) && !us.contains(&up.as_str()) { format!("{up}.JK") } else { up };
    Some((sym.clone(), sym))
}

pub async fn kuotasi(http: &reqwest::Client, sym: &str) -> Result<(f64, f64, String, Vec<f64>), String> {
    let url = format!("https://query1.finance.yahoo.com/v8/finance/chart/{}?range=1mo&interval=1d", urlencoding::encode(sym));
    let v: Value = http.get(url).timeout(Duration::from_secs(12)).header("user-agent", "Mozilla/5.0").send().await.map_err(|e| format!("tidak ada internet? {e}"))?.json().await.map_err(|e| format!("balasan tak terbaca: {e}"))?;
    let r = &v["chart"]["result"][0];
    if r.is_null() {
        return Err(format!("simbol '{sym}' tidak ditemukan"));
    }
    let m = &r["meta"];
    let price = m["regularMarketPrice"].as_f64().ok_or("harga tidak tersedia")?;
    let closes: Vec<f64> = r["indicators"]["quote"][0]["close"].as_array().map(|a| a.iter().filter_map(|x| x.as_f64()).collect()).unwrap_or_default();
    // penutupan sebelumnya = penutupan hari bursa terakhir sebelum harga sekarang (bukan awal rentang)
    let prev = if closes.len() >= 2 { closes[closes.len() - 2] } else { m["previousClose"].as_f64().unwrap_or(price) };
    Ok((price, prev, m["currency"].as_str().unwrap_or("").to_string(), closes))
}

fn fmt_num(v: f64) -> String {
    if v.abs() >= 1000.0 { format!("{:.0}", v).as_bytes().rchunks(3).rev().map(|c| std::str::from_utf8(c).unwrap_or("")).collect::<Vec<_>>().join(".") } else { format!("{v:.2}") }
}

/// Harga + perubahan + grafik 30 hari.
pub async fn harga(http: &reqwest::Client, teks: &str) -> String {
    let Some((sym, nama)) = simbol(teks) else { return "Sebut asetnya: harga bbca · harga bitcoin · kurs dolar · harga emas · harga ihsg · harga AAPL".into() };
    match kuotasi(http, &sym).await {
        Ok((p, prev, cur, closes)) => {
            let d = if prev > 0.0 { (p - prev) / prev * 100.0 } else { 0.0 };
            let chart = serde_json::json!({"t": format!("{nama} · 30 hari"), "u": cur, "v": closes});
            format!("📈 {nama} ({sym}): {} {cur} · {}{:.2}% dari penutupan sebelumnya\nCHART {chart}\nSumber: Yahoo Finance (tunda ±15 menit; bukan saran investasi)", fmt_num(p), if d >= 0.0 { "+" } else { "" }, d)
        }
        Err(e) => format!("Gagal mengambil harga {sym}: {e}"),
    }
}

/// pantau <aset> di atas|di bawah <angka> | pantau (daftar) | pantau hapus N
pub async fn pantau(http: &reqwest::Client, teks: &str) -> String {
    let Some(c) = db() else { return "database tidak tersedia".into() };
    let l = teks.to_lowercase();
    if let Some(n) = l.split_whitespace().skip_while(|w| *w != "hapus").nth(1).and_then(|x| x.trim_start_matches('#').parse::<i64>().ok()) {
        return format!("🗑 {} pantauan dihapus.", c.execute("DELETE FROM pantau WHERE id=?1", [n]).unwrap_or(0));
    }
    let arah = if l.contains("bawah") || l.contains("turun") || l.contains("kurang dari") { "bawah" } else { "atas" };
    let batas: Option<f64> = l.replace('.', "").replace(',', ".").split_whitespace().rev().find_map(|w| w.trim_matches(|ch: char| !ch.is_ascii_digit() && ch != '.').parse().ok());
    let clean: String = teks.split_whitespace().filter(|w| !["pantau", "pantauan", "harga", "kalau", "jika", "di", "atas", "bawah", "ingatkan", "saya", "tolong", "nova", "coba"].contains(&w.to_lowercase().as_str()) && w.replace('.', "").replace(',', "").parse::<f64>().is_err()).collect::<Vec<_>>().join(" ");
    if let (Some(b), Some((sym, nama))) = (batas, simbol(&clean).or_else(|| simbol(teks))) {
        let _ = c.execute("INSERT INTO pantau(sym, arah, batas) VALUES(?1,?2,?3)", rusqlite::params![sym, arah, b]);
        let now = kuotasi(http, &sym).await.map(|x| format!(" Harga sekarang {}.", fmt_num(x.0))).unwrap_or_default();
        return format!("👁 Memantau {nama} ({sym}): kabari saat di {arah} {}.{now} Dicek tiap 15 menit.", fmt_num(b));
    }
    let Ok(mut st) = c.prepare("SELECT id, sym, arah, batas FROM pantau ORDER BY id") else { return String::new() };
    let rows: Vec<(i64, String, String, f64)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).map(|r| r.flatten().collect()).unwrap_or_default();
    if rows.is_empty() {
        return "Belum ada pantauan. Contoh: pantau bbca di bawah 9000 · pantau bitcoin di atas 120000 · pantau kurs dolar di atas 17000".into();
    }
    format!("👁 Pantauan harga:\n{}\nHapus: pantau hapus <nomor>", rows.iter().map(|(i, s, a, b)| format!("#{i} {s} di {a} {}", fmt_num(*b))).collect::<Vec<_>>().join("\n"))
}

/// pengingat cuaca: cuacapantau <kota> | cuacapantau hapus <kota> | cuacapantau
pub fn cuaca_pantau(teks: &str) -> String {
    let Some(c) = db() else { return "database tidak tersedia".into() };
    let l = teks.to_lowercase();
    let kota: String = l.split_whitespace().filter(|w| !["cuacapantau", "pantau", "cuaca", "ingatkan", "kalau", "jika", "hujan", "panas", "di", "kota", "saya", "tolong", "nova", "coba", "hapus"].contains(w)).collect::<Vec<_>>().join(" ");
    if l.contains("hapus") {
        return format!("🗑 {} kota dihapus dari pemantau cuaca.", c.execute("DELETE FROM cuaca_pantau WHERE kota=?1", [&kota]).unwrap_or(0));
    }
    if kota.is_empty() {
        let Ok(mut st) = c.prepare("SELECT kota FROM cuaca_pantau") else { return String::new() };
        let v: Vec<String> = st.query_map([], |r| r.get(0)).map(|r| r.flatten().collect()).unwrap_or_default();
        return if v.is_empty() { "Belum ada kota. Contoh: ingatkan kalau hujan di bandung".into() } else { format!("☔ Pengingat cuaca aktif untuk: {}. Hapus: cuacapantau hapus <kota>", v.join(", ")) };
    }
    let _ = c.execute("INSERT OR IGNORE INTO cuaca_pantau(kota) VALUES(?1)", [&kota]);
    format!("☔ Kupantau cuaca {kota}: ringkasan pagi tiap hari (bila berpotensi hujan ≥60% atau panas ≥35°C) dan peringatan 2 jam sebelum hujan.")
}

/// Latar belakang: cek ambang harga tiap 15 menit dan cuaca tiap 30 menit.
pub fn start(tx: Tx) {
    std::thread::spawn(move || {
        let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() else { return };
        let http = crate::research::client(15);
        let mut n = 0u64;
        loop {
            std::thread::sleep(Duration::from_secs(900));
            n += 1;
            rt.block_on(async {
                if let Some(c) = db() {
                    let rows: Vec<(i64, String, String, f64, i64)> = c.prepare("SELECT id, sym, arah, batas, alert_ts FROM pantau").ok().and_then(|mut s| s.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))).ok().map(|r| r.flatten().collect())).unwrap_or_default();
                    let now = chrono::Utc::now().timestamp();
                    for (id, sym, arah, batas, last) in rows {
                        if now - last < 6 * 3600 {
                            continue;
                        }
                        if let Ok((p, _, cur, _)) = kuotasi(&http, &sym).await {
                            if (arah == "atas" && p >= batas) || (arah == "bawah" && p <= batas) {
                                let _ = c.execute("UPDATE pantau SET alert_ts=?1 WHERE id=?2", rusqlite::params![now, id]);
                                let m = format!("📈 {sym} {} {cur}: sudah di {arah} ambang {}.", fmt_num(p), fmt_num(batas));
                                crate::notify::toast("NOVA - pantauan harga", &m);
                                tx.notice(m);
                            }
                        }
                    }
                }
                if n % 2 == 0 {
                    cek_cuaca(&http, &tx).await;
                }
            });
        }
    });
}

async fn cek_cuaca(http: &reqwest::Client, tx: &Tx) {
    let Some(c) = db() else { return };
    let kotas: Vec<(String, String, i64)> = c.prepare("SELECT kota, hari, hujan_ts FROM cuaca_pantau").ok().and_then(|mut s| s.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).ok().map(|r| r.flatten().collect())).unwrap_or_default();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    for (kota, hari, hujan_ts) in kotas {
        let Ok(g) = http.get(format!("https://geocoding-api.open-meteo.com/v1/search?name={}&count=1", urlencoding::encode(&kota))).send().await else { continue };
        let Ok(g) = g.json::<Value>().await else { continue };
        let Some(p) = g["results"].get(0) else { continue };
        let (lat, lon) = (p["latitude"].as_f64().unwrap_or(0.0), p["longitude"].as_f64().unwrap_or(0.0));
        let Ok(f) = http.get(format!("https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}&hourly=precipitation_probability&daily=precipitation_probability_max,temperature_2m_max&timezone=auto&forecast_days=1")).send().await else { continue };
        let Ok(f) = f.json::<Value>().await else { continue };
        let (rain, tmax) = (f["daily"]["precipitation_probability_max"][0].as_f64().unwrap_or(0.0), f["daily"]["temperature_2m_max"][0].as_f64().unwrap_or(0.0));
        if hari != today && chrono::Timelike::hour(&chrono::Local::now()) >= 5 {
            let _ = c.execute("UPDATE cuaca_pantau SET hari=?1 WHERE kota=?2", rusqlite::params![today, kota]);
            if rain >= 60.0 || tmax >= 35.0 {
                let m = format!("{} {kota} hari ini: {}{}", if rain >= 60.0 { "☔" } else { "🌡" }, if rain >= 60.0 { format!("peluang hujan {rain:.0}% — bawa payung/jas hujan. ") } else { String::new() }, if tmax >= 35.0 { format!("panas hingga {tmax:.0}°C — minum yang cukup.") } else { String::new() });
                crate::notify::toast("NOVA - cuaca", &m);
                tx.notice(m);
            }
        }
        // hujan dalam 2 jam ke depan
        let h = chrono::Timelike::hour(&chrono::Local::now()) as usize;
        let soon = (h + 1..=h + 2).filter_map(|i| f["hourly"]["precipitation_probability"][i.min(23)].as_f64()).fold(0.0f64, f64::max);
        let now = chrono::Utc::now().timestamp();
        if soon >= 70.0 && now - hujan_ts > 3 * 3600 {
            let _ = c.execute("UPDATE cuaca_pantau SET hujan_ts=?1 WHERE kota=?2", rusqlite::params![now, kota]);
            let m = format!("☔ Hujan diperkirakan di {kota} dalam 1-2 jam ({soon:.0}%). Jemuran dan payung!");
            crate::notify::toast("NOVA - cuaca", &m);
            tx.notice(m);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn symbols() {
        assert_eq!(simbol("harga bbca").unwrap().0, "BBCA.JK");
        assert_eq!(simbol("kurs dolar").unwrap().0, "IDR=X");
        assert_eq!(simbol("harga bitcoin").unwrap().0, "BTC-USD");
        assert_eq!(simbol("harga AAPL").unwrap().0, "AAPL");
        assert_eq!(fmt_num(1788055.0), "1.788.055");
    }
}
