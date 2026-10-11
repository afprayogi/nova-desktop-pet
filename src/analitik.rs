//! Analitik tanpa pustaka berat: (1) K-Means untuk mengelompokkan hari menjadi tipe (santai/normal/padat-lembur) dan
//! menandai hari tidak biasa, (2) deteksi anomali CPU/RAM dengan median + MAD, (3) peramalan tren disk penuh dengan regresi
//! linear dari sampel harian. Sampel metrik diambil tiap 10 menit di latar dan disimpan 60 hari.
use chrono::Timelike;
use std::time::Duration;
use sysinfo::{Disks, System};

fn db() -> Option<rusqlite::Connection> {
    let c = crate::assistant::db().ok()?;
    c.execute_batch("CREATE TABLE IF NOT EXISTS metrik(ts INT PRIMARY KEY, cpu REAL, ram REAL, disk_used REAL, disk_total REAL);").ok()?;
    Some(c)
}

fn median(v: &mut Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    if v.is_empty() {
        0.0
    } else {
        v[v.len() / 2]
    }
}

/// (median, MAD) dari nilai historis.
fn robust(vals: &[f64]) -> (f64, f64) {
    let mut a = vals.to_vec();
    let m = median(&mut a);
    let mut d: Vec<f64> = vals.iter().map(|x| (x - m).abs()).collect();
    (m, median(&mut d).max(1.0))
}

/// Anomali bila nilai melebihi median + 4*MAD dan melewati lantai mutlak.
pub fn is_anomaly(history: &[f64], now: f64, floor: f64) -> Option<f64> {
    if history.len() < 50 {
        return None;
    }
    let (m, mad) = robust(history);
    (now > m + 4.0 * mad && now > floor).then_some(m)
}

// ---------------- sampling & anomali ----------------

pub fn start(tx: crate::engine::Tx) {
    std::thread::spawn(move || {
        let mut sys = System::new();
        let mut disks = Disks::new_with_refreshed_list();
        let mut last_alert: std::collections::HashMap<&str, std::time::Instant> = Default::default();
        sys.refresh_cpu_usage();
        loop {
            std::thread::sleep(Duration::from_secs(600));
            sys.refresh_cpu_usage();
            sys.refresh_memory();
            disks.refresh();
            let cpu = sys.global_cpu_usage() as f64;
            let ram = if sys.total_memory() > 0 { sys.used_memory() as f64 * 100.0 / sys.total_memory() as f64 } else { 0.0 };
            let (used, total) = disks
                .list()
                .iter()
                .find(|d| d.mount_point().to_string_lossy().starts_with("C:"))
                .or_else(|| disks.list().first())
                .map(|d| ((d.total_space() - d.available_space()) as f64 / 1e9, d.total_space() as f64 / 1e9))
                .unwrap_or((0.0, 1.0));
            let Some(c) = db() else { continue };
            let ts = chrono::Utc::now().timestamp();
            let hist = |col: &str| -> Vec<f64> {
                c.prepare(&format!("SELECT {col} FROM metrik WHERE ts > ?1")).ok().and_then(|mut s| s.query_map([ts - 7 * 86400], |r| r.get::<_, f64>(0)).ok().map(|r| r.flatten().collect())).unwrap_or_default()
            };
            for (name, label, now, floor) in [("cpu", "CPU", cpu, 85.0), ("ram", "RAM", ram, 90.0)] {
                if let Some(typ) = is_anomaly(&hist(name), now, floor) {
                    if last_alert.get(name).map_or(true, |t| t.elapsed() > Duration::from_secs(2 * 3600)) {
                        last_alert.insert(name, std::time::Instant::now());
                        let msg = format!("{label} {now:.0}% tidak biasa (biasanya ±{typ:.0}%). Cek proses terberat: ketik 'aplikasi apa yang paling berat'.");
                        crate::notify::toast("SENTRY - anomali", &msg);
                        crate::adegan::play(&tx, vec![(2, format!("⚠ {msg}"))]);
                    }
                }
            }
            let _ = c.execute("INSERT OR REPLACE INTO metrik VALUES(?1,?2,?3,?4,?5)", rusqlite::params![ts, cpu, ram, used, total]);
            let _ = c.execute("DELETE FROM metrik WHERE ts < ?1", [ts - 60 * 86400]);
        }
    });
}

// ---------------- peramalan disk ----------------

/// Regresi linear (hari -> GB terpakai). Mengembalikan (kemiringan GB/hari, terpakai sekarang, total).
fn disk_trend() -> Option<(f64, f64, f64)> {
    let c = db()?;
    // satu titik per hari (terakhir) agar tidak berat sebelah
    let mut st = c.prepare("SELECT ts/86400, disk_used, disk_total FROM metrik WHERE ts > ?1 GROUP BY ts/86400 HAVING ts = MAX(ts) ORDER BY ts").ok()?;
    let pts: Vec<(f64, f64, f64)> = st.query_map([chrono::Utc::now().timestamp() - 30 * 86400], |r| Ok((r.get::<_, i64>(0)? as f64, r.get(1)?, r.get(2)?))).ok()?.flatten().collect();
    if pts.len() < 3 {
        return None;
    }
    let n = pts.len() as f64;
    let (sx, sy): (f64, f64) = (pts.iter().map(|p| p.0).sum(), pts.iter().map(|p| p.1).sum());
    let (sxx, sxy): (f64, f64) = (pts.iter().map(|p| p.0 * p.0).sum(), pts.iter().map(|p| p.0 * p.1).sum());
    let den = n * sxx - sx * sx;
    if den.abs() < 1e-9 {
        return None;
    }
    let (_, used, total) = *pts.last()?;
    Some(((n * sxy - sx * sy) / den, used, total))
}

pub fn ramalan_disk() -> String {
    match disk_trend() {
        Some((slope, used, total)) => {
            let free = total - used;
            if slope > 0.05 {
                format!("Disk: terpakai {used:.0}/{total:.0} GB, bertambah ±{slope:.1} GB/hari → penuh dalam ±{:.0} hari.", (free / slope).max(0.0))
            } else {
                format!("Disk: terpakai {used:.0}/{total:.0} GB, sisa {free:.1} GB, tren stabil (belum ada pertumbuhan berarti).")
            }
        }
        None => {
            let (used, total) = Disks::new_with_refreshed_list().list().first().map(|d| ((d.total_space() - d.available_space()) as f64 / 1e9, d.total_space() as f64 / 1e9)).unwrap_or((0.0, 0.0));
            format!("Disk: terpakai {used:.0}/{total:.0} GB. Ramalan tren butuh ≥3 hari data (sampel tiap 10 menit sedang dikumpulkan).")
        }
    }
}

// ---------------- K-Means tipe hari ----------------

type Feat = [f64; 4];

fn kmeans(data: &[Feat], k: usize) -> (Vec<usize>, Vec<Feat>) {
    let dist = |a: &Feat, b: &Feat| a.iter().zip(b).map(|(x, y)| (x - y).powi(2)).sum::<f64>();
    // inisialisasi farthest-first (deterministik)
    let mut cents: Vec<Feat> = vec![data[0]];
    while cents.len() < k {
        let far = data.iter().max_by(|a, b| cents.iter().map(|c| dist(a, c)).fold(f64::MAX, f64::min).total_cmp(&cents.iter().map(|c| dist(b, c)).fold(f64::MAX, f64::min))).unwrap();
        cents.push(*far);
    }
    let mut asg = vec![0usize; data.len()];
    for _ in 0..30 {
        for (i, d) in data.iter().enumerate() {
            asg[i] = (0..k).min_by(|a, b| dist(d, &cents[*a]).total_cmp(&dist(d, &cents[*b]))).unwrap();
        }
        for (j, c) in cents.iter_mut().enumerate() {
            let m: Vec<&Feat> = data.iter().zip(&asg).filter(|(_, a)| **a == j).map(|(d, _)| d).collect();
            if !m.is_empty() {
                for f in 0..4 {
                    c[f] = m.iter().map(|d| d[f]).sum::<f64>() / m.len() as f64;
                }
            }
        }
    }
    (asg, cents)
}

/// Fitur per hari dari tabel usage: [jam aktif, jam mulai, jam akhir, porsi malam].
fn day_features() -> Vec<(String, Feat)> {
    let Ok(c) = crate::assistant::db() else { return vec![] };
    let Ok(mut st) = c.prepare("SELECT day, hour, SUM(secs) FROM usage WHERE day >= ?1 GROUP BY day, hour ORDER BY day, hour") else { return vec![] };
    let cutoff = (chrono::Local::now() - chrono::Duration::days(45)).format("%Y-%m-%d").to_string();
    let rows: Vec<(String, i64, i64)> = st.query_map([cutoff], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).map(|r| r.flatten().collect()).unwrap_or_default();
    let mut out: Vec<(String, Feat)> = vec![];
    let mut i = 0;
    while i < rows.len() {
        let day = rows[i].0.clone();
        let mut j = i;
        let (mut total, mut eve, mut first, mut last) = (0f64, 0f64, 99f64, 0f64);
        while j < rows.len() && rows[j].0 == day {
            let (h, s) = (rows[j].1 as f64, rows[j].2 as f64);
            total += s;
            if h >= 19.0 {
                eve += s;
            }
            first = first.min(h);
            last = last.max(h);
            j += 1;
        }
        if total >= 1800.0 {
            out.push((day, [total / 3600.0, first, last, eve / total]));
        }
        i = j;
    }
    out
}

pub fn tipe_hari() -> String {
    let days = day_features();
    if days.len() < 5 {
        return format!("Tipe hari: butuh ≥5 hari data penggunaan (baru {}). Data dikumpulkan otomatis tiap hari.", days.len());
    }
    // normalisasi z-score per fitur
    let n = days.len() as f64;
    let (mut mu, mut sd) = ([0f64; 4], [1f64; 4]);
    for f in 0..4 {
        mu[f] = days.iter().map(|d| d.1[f]).sum::<f64>() / n;
        sd[f] = (days.iter().map(|d| (d.1[f] - mu[f]).powi(2)).sum::<f64>() / n).sqrt().max(1e-6);
    }
    let z: Vec<Feat> = days.iter().map(|d| [(d.1[0] - mu[0]) / sd[0], (d.1[1] - mu[1]) / sd[1], (d.1[2] - mu[2]) / sd[2], (d.1[3] - mu[3]) / sd[3]]).collect();
    let k = 3.min(days.len() / 2).max(2);
    let (asg, cents) = kmeans(&z, k);
    // beri nama: urut dari rata-rata jam aktif (kolom 0 sebelum normalisasi)
    let mean_hours = |j: usize| -> f64 {
        let m: Vec<f64> = days.iter().zip(&asg).filter(|(_, a)| **a == j).map(|(d, _)| d.1[0]).collect();
        if m.is_empty() { 0.0 } else { m.iter().sum::<f64>() / m.len() as f64 }
    };
    let mean_last = |j: usize| -> f64 {
        let m: Vec<f64> = days.iter().zip(&asg).filter(|(_, a)| **a == j).map(|(d, _)| d.1[2]).collect();
        if m.is_empty() { 0.0 } else { m.iter().sum::<f64>() / m.len() as f64 }
    };
    let mut order: Vec<usize> = (0..k).collect();
    order.sort_by(|a, b| mean_hours(*a).total_cmp(&mean_hours(*b)));
    let name = |j: usize| -> &'static str {
        let rank = order.iter().position(|x| *x == j).unwrap_or(0);
        if rank == k - 1 && (mean_last(j) >= 22.0 || k == 2) { "padat/lembur" } else if rank == k - 1 { "produktif" } else if rank == 0 { "santai" } else { "normal" }
    };
    let (today, i_today) = (chrono::Local::now().format("%Y-%m-%d").to_string(), days.iter().position(|d| d.0 == chrono::Local::now().format("%Y-%m-%d").to_string()));
    let mut s = format!("Tipe hari (K-Means, {} hari): ", days.len());
    s.push_str(&(0..k).map(|j| format!("{} ×{} (rata-rata {:.1} jam)", name(j), asg.iter().filter(|a| **a == j).count(), mean_hours(j))).collect::<Vec<_>>().join(", "));
    if let Some(i) = i_today {
        let d = |a: &Feat, b: &Feat| a.iter().zip(b).map(|(x, y)| (x - y).powi(2)).sum::<f64>().sqrt();
        let dists: Vec<f64> = z.iter().zip(&asg).map(|(p, a)| d(p, &cents[*a])).collect();
        let mean = dists.iter().sum::<f64>() / n;
        s.push_str(&format!("\nHari ini ({today}) tergolong '{}'", name(asg[i])));
        if dists[i] > mean * 2.0 && dists.len() >= 6 {
            s.push_str(" — pola hari ini TIDAK BIASA dibanding hari-hari sebelumnya.");
        } else {
            s.push('.');
        }
        let h = chrono::Local::now().hour();
        if h >= 22 && days[i].1[0] > mu[0] + sd[0] {
            s.push_str(" Kamu lebih lama di depan laptop dari biasanya.");
        }
    }
    s
}

/// Laporan analitik gabungan.
pub fn laporan() -> String {
    let p = crate::hewan::pola();
    let mut idx: Vec<usize> = (0..24).collect();
    idx.sort_by(|a, b| p.per_jam[*b].total_cmp(&p.per_jam[*a]));
    let jam: Vec<String> = idx.iter().take(3).filter(|i| p.per_jam[**i] > 1.0).map(|i| format!("{:02}:00", i)).collect();
    format!("{}\n{}\nJam produktif yang diperkirakan besok: {}\nAnomali CPU/RAM: dipantau tiap 10 menit (median + 4×MAD, lantai CPU 85% / RAM 90%).", tipe_hari(), ramalan_disk(), if jam.is_empty() { "belum cukup data".into() } else { jam.join(", ") })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn anomaly_needs_history_and_floor() {
        let hist: Vec<f64> = (0..100).map(|i| 50.0 + (i % 5) as f64).collect();
        assert!(is_anomaly(&hist, 95.0, 85.0).is_some());
        assert!(is_anomaly(&hist, 60.0, 85.0).is_none());
        assert!(is_anomaly(&hist[..10], 95.0, 85.0).is_none());
    }
    #[test]
    fn kmeans_separates_clusters() {
        let mut d: Vec<Feat> = vec![];
        for i in 0..6 {
            d.push([1.0 + i as f64 * 0.05, 0.0, 0.0, 0.0]);
            d.push([9.0 + i as f64 * 0.05, 0.0, 0.0, 0.0]);
        }
        let (a, _) = kmeans(&d, 2);
        assert_ne!(a[0], a[1]);
        assert_eq!(a[0], a[2]);
        assert_eq!(a[1], a[3]);
    }
}
