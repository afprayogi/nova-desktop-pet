//! Prediksi baterai dengan regresi online per konteks: laju pemakaian (%/jam) dipelajari terpisah untuk tiap
//! mode kerja (Coding, Browsing, Santai, ...) dan saat mengisi daya. Sampel disimpan di disk (kesehatan.db).
use crate::engine::Tx;
use rusqlite::Connection;
use std::time::Duration;

#[derive(Clone, Copy, Debug)]
pub struct Power {
    pub pct: f32,
    pub ac: bool,
    /// Perkiraan bawaan Windows (detik) bila tersedia.
    pub os_secs: Option<u32>,
}

/// None = tidak ada baterai (PC desktop) atau tidak terbaca.
#[cfg(windows)]
pub fn read() -> Option<Power> {
    use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    let mut s = SYSTEM_POWER_STATUS { ACLineStatus: 0, BatteryFlag: 0, BatteryLifePercent: 0, SystemStatusFlag: 0, BatteryLifeTime: 0, BatteryFullLifeTime: 0 };
    if unsafe { GetSystemPowerStatus(&mut s) } == 0 || s.BatteryFlag & 128 != 0 || s.BatteryLifePercent > 100 {
        return None;
    }
    Some(Power { pct: s.BatteryLifePercent as f32, ac: s.ACLineStatus == 1, os_secs: (s.BatteryLifeTime != u32::MAX).then_some(s.BatteryLifeTime) })
}

#[cfg(not(windows))]
pub fn read() -> Option<Power> {
    None
}

fn db() -> rusqlite::Result<Connection> {
    let c = Connection::open(crate::config::data_dir().join("kesehatan.db"))?;
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS bat(ts INTEGER, pct REAL, ac INT, ctx TEXT);
         CREATE TABLE IF NOT EXISTS rate(ctx TEXT PRIMARY KEY, ewma REAL, n INT);",
    )?;
    Ok(c)
}

/// Pembaruan laju (EWMA setelah 5 pengamatan pertama berupa rata-rata berjalan).
pub fn update_rate(old: Option<(f32, i64)>, obs: f32) -> (f32, i64) {
    match old {
        None => (obs, 1),
        Some((e, n)) if n < 5 => ((e * n as f32 + obs) / (n as f32 + 1.0), n + 1),
        Some((e, n)) => (0.9 * e + 0.1 * obs, n + 1),
    }
}

/// Prediksi menit tersisa: pct / (laju %/jam) * 60.
pub fn minutes(pct: f32, rate_per_h: f32) -> Option<u32> {
    (rate_per_h > 0.3).then(|| (pct / rate_per_h * 60.0) as u32)
}

fn get_rate(c: &Connection, ctx: &str) -> Option<(f32, i64)> {
    c.query_row("SELECT ewma, n FROM rate WHERE ctx=?1", [ctx], |r| Ok((r.get::<_, f64>(0)? as f32, r.get::<_, i64>(1)?))).ok()
}

fn set_rate(c: &Connection, ctx: &str, v: (f32, i64)) {
    let _ = c.execute("INSERT INTO rate VALUES(?1,?2,?3) ON CONFLICT(ctx) DO UPDATE SET ewma=?2, n=?3", (ctx, v.0 as f64, v.1));
}

fn fmt_min(m: u32) -> String {
    if m >= 60 { format!("{}j {}m", m / 60, m % 60) } else { format!("{m} menit") }
}

/// Laporan + prediksi untuk alat `baterai`.
pub fn report() -> String {
    let Some(p) = read() else { return "Tidak ada baterai terdeteksi (PC desktop atau tidak terbaca).".into() };
    let ctx = crate::konteks::current().0.label();
    let Ok(c) = db() else { return format!("Baterai {:.0}%", p.pct) };
    let mut s = format!("Baterai {:.0}% · {}", p.pct, if p.ac { "mengisi/terhubung charger" } else { "memakai baterai" });
    if p.ac {
        match get_rate(&c, "charge").and_then(|(r, n)| (n >= 2).then_some(r)) {
            Some(r) if p.pct < 100.0 => s.push_str(&format!("\nPrediksi penuh: ~{} (laju isi {r:.0}%/jam, dipelajari dari {} pengamatan)", fmt_min(((100.0 - p.pct) / r * 60.0) as u32), get_rate(&c, "charge").map(|x| x.1).unwrap_or(0))),
            _ => s.push_str("\nBelum cukup data pengisian untuk memprediksi."),
        }
    } else {
        let (rate, src) = match (get_rate(&c, ctx), get_rate(&c, "semua")) {
            (Some((r, n)), _) if n >= 3 => (Some(r), format!("konteks {ctx}")),
            (_, Some((r, n))) if n >= 3 => (Some(r), "rata-rata semua konteks".to_string()),
            _ => (None, String::new()),
        };
        match rate.and_then(|r| minutes(p.pct, r).map(|m| (r, m))) {
            Some((r, m)) => s.push_str(&format!("\nPrediksi habis: ~{} (laju {r:.1}%/jam, {src})", fmt_min(m))),
            None => s.push_str("\nModel belum punya cukup data; lepas charger beberapa menit agar ia belajar."),
        }
        if let Some(o) = p.os_secs {
            s.push_str(&format!("\nPerkiraan Windows: ~{}", fmt_min(o / 60)));
        }
    }
    s
}

/// Pengambil sampel tiap menit + belajar laju + peringatan.
pub async fn run(tx: Tx) {
    let Ok(c) = db() else { return };
    let mut prev_ac: Option<bool> = None;
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
        let Some(p) = read() else { continue };
        let ctx = crate::konteks::current().0.label().to_string();
        let now = chrono::Local::now().timestamp();
        let _ = c.execute("INSERT INTO bat VALUES(?1,?2,?3,?4)", (now, p.pct as f64, p.ac as i64, &ctx));
        let _ = c.execute("DELETE FROM bat WHERE ts < ?1", [now - 14 * 86400]);

        // Belajar: bandingkan dengan sampel tertua dalam 20 menit terakhir pada status daya yang sama.
        let old: Option<(i64, f64)> = c
            .query_row("SELECT ts, pct FROM bat WHERE ts >= ?1 AND ts < ?2 AND ac = ?3 ORDER BY ts LIMIT 1", (now - 1200, now - 300, p.ac as i64), |r| Ok((r.get(0)?, r.get(1)?)))
            .ok();
        if let Some((t0, p0)) = old {
            let hours = (now - t0) as f32 / 3600.0;
            let delta = if p.ac { p.pct - p0 as f32 } else { p0 as f32 - p.pct };
            if delta >= 1.0 && hours > 0.0 {
                let obs = delta / hours;
                let keys: Vec<&str> = if p.ac { vec!["charge"] } else { vec![ctx.as_str(), "semua"] };
                for k in keys {
                    let v = update_rate(get_rate(&c, k), obs);
                    set_rate(&c, k, v);
                }
            }
        }

        // Peringatan (sekali per ambang per hari).
        let day = chrono::Local::now().format("%Y-%m-%d").to_string();
        let mut notes: Vec<String> = vec![];
        if !p.ac {
            let rate = get_rate(&c, &ctx).filter(|x| x.1 >= 3).or_else(|| get_rate(&c, "semua")).map(|x| x.0);
            let left = rate.and_then(|r| minutes(p.pct, r));
            for th in [20.0f32, 10.0] {
                if p.pct <= th && crate::assistant::once_key(&format!("{day}bat{th}")) {
                    notes.push(format!("Baterai tinggal {:.0}%{}. Colokkan charger.", p.pct, left.map(|m| format!(" (prediksi habis ±{})", fmt_min(m))).unwrap_or_default()));
                }
            }
            if let Some(m) = left {
                if m <= 25 && p.pct > 10.0 && crate::assistant::once_key(&format!("{day}batp{}", p.pct as i32 / 5)) {
                    notes.push(format!("Prediksiku: baterai habis sekitar {} lagi pada laju sekarang. Simpan pekerjaanmu.", fmt_min(m)));
                }
            }
        } else if p.pct >= 95.0 && prev_ac == Some(true) && crate::assistant::once_key(&format!("{day}batfull")) {
            notes.push("Baterai hampir penuh. Cabut charger untuk menjaga kesehatan baterai.".into());
        }
        prev_ac = Some(p.ac);
        for n in notes {
            crate::notify::toast("NOVA - baterai", &n);
            crate::adegan::play(&tx, vec![(2, n)]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rate_learning_and_prediction() {
        let mut v = None;
        for obs in [10.0, 12.0, 11.0] {
            v = Some(update_rate(v, obs));
        }
        let (e, n) = v.unwrap();
        assert_eq!(n, 3);
        assert!((e - 11.0).abs() < 0.01);
        assert_eq!(minutes(50.0, 10.0), Some(300));
        assert_eq!(minutes(50.0, 0.0), None);
        let (e2, _) = update_rate(Some((e, 6)), 21.0);
        assert!(e2 > e && e2 < 21.0); // EWMA bergerak pelan
    }
}
