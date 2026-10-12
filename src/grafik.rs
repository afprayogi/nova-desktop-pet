//! Grafik pemakaian laptop hari ini dari data pemantauan (`kesehatan.db`, tabel h: ts, cpu, ram%). Keluaran memakai baris `CHART {json}`
//! yang sudah digambar oleh obrolan web dan pop-up (sama seperti grafik harga).
use rusqlite::Connection;

/// Rata-ratakan `v` menjadi paling banyak `n` titik.
pub fn ringkas(v: &[f64], n: usize) -> Vec<f64> {
    if v.len() <= n || n == 0 {
        return v.to_vec();
    }
    (0..n).map(|i| {
        let (a, b) = (i * v.len() / n, ((i + 1) * v.len() / n).max(i * v.len() / n + 1));
        v[a..b].iter().sum::<f64>() / (b - a) as f64
    }).collect()
}

fn jam(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0).map(|d| d.with_timezone(&chrono::Local).format("%H:%M").to_string()).unwrap_or_default()
}

pub fn hari_ini() -> String {
    let Ok(c) = Connection::open(crate::config::data_dir().join("kesehatan.db")) else { return "Data pemantauan belum ada.".into() };
    let mulai = chrono::Local::now().date_naive().and_hms_opt(0, 0, 0).and_then(|d| d.and_local_timezone(chrono::Local).single()).map(|d| d.timestamp()).unwrap_or(0);
    // data pemantauan hanya disimpan 24 jam terakhir: "hari ini" = sejak 00:00 atau 24 jam terakhir bila datanya lebih sedikit
    let rows: Vec<(i64, f64, f64)> = c
        .prepare("SELECT ts, cpu, ram FROM h WHERE ts >= ?1 ORDER BY ts")
        .and_then(|mut st| st.query_map([mulai], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).map(|r| r.flatten().collect()))
        .unwrap_or_default();
    if rows.len() < 3 {
        return "Belum cukup data pemantauan hari ini (NOVA mencatat tiap beberapa detik saat berjalan).".into();
    }
    let (cpu, ram): (Vec<f64>, Vec<f64>) = rows.iter().map(|r| (r.1, r.2)).unzip();
    let stat = |v: &[f64]| {
        let (i, mx) = v.iter().cloned().enumerate().fold((0, 0.0), |a, (i, x)| if x > a.1 { (i, x) } else { a });
        (v.iter().sum::<f64>() / v.len() as f64, mx, jam(rows[i].0))
    };
    let (ra, rm, rj) = stat(&ram);
    let (ca, cm, cj) = stat(&cpu);
    let nilai = if ra > 85.0 { "TINGGI: RAM hampir penuh sepanjang hari; tutup aplikasi berat atau tambah RAM." } else if ra > 70.0 { "cukup tinggi; pantau aplikasi yang paling besar memakai memori." } else { "sehat." };
    let judul = |n: &str, v: &[f64]| serde_json::json!({"t": format!("{n} hari ini (% , {} → {})", jam(rows[0].0), jam(rows[rows.len() - 1].0)), "u": "%", "v": ringkas(v, 60).iter().map(|x| (x * 10.0).round() / 10.0).collect::<Vec<_>>()});
    format!(
        "📊 Pemakaian hari ini ({} sampel): RAM rata-rata {:.0}% (puncak {:.0}% pukul {}), CPU rata-rata {:.0}% (puncak {:.0}% pukul {}). Kesehatan RAM {}\nCHART {}\nCHART {}",
        rows.len(), ra, rm, rj, ca, cm, cj, nilai, judul("RAM", &ram), judul("CPU", &cpu)
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn downsamples_by_average() {
        assert_eq!(super::ringkas(&[1.0, 3.0, 5.0, 7.0], 2), vec![2.0, 6.0]);
        assert_eq!(super::ringkas(&[1.0, 2.0], 5), vec![1.0, 2.0]);
    }
}
