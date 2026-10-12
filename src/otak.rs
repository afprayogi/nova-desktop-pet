//! Peta otak NOVA: seberapa jauh ingatan dan pemahaman sudah tumbuh, plus pembelajaran berkelanjutan harian.
//! Tiap dimensi punya target; "tingkat otak" = rata-rata kemajuan. Snapshot harian disimpan agar pertumbuhan terlihat di dasbor.
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Default, Debug)]
pub struct Snap {
    pub hari: String,
    pub fakta: i64,
    pub contoh: i64,
    pub pengetahuan: i64,
    pub graf_simpul: i64,
    pub graf_sisi: i64,
    pub cache: i64,
    pub sampel: i64,
    pub akurasi: i64,
    pub lagu: i64,
}

/// (nama, nilai sekarang, target tahap berikutnya)
pub fn dimensi(s: &Snap) -> Vec<(&'static str, i64, i64)> {
    vec![
        ("Fakta tentang Bos", s.fakta, 100),
        ("Contoh niat dipelajari", s.contoh, 500),
        ("Pengetahuan (potongan dokumen)", s.pengetahuan, 3000),
        ("Graf ingatan (simpul)", s.graf_simpul, 2000),
        ("Memori jawaban Gemini", s.cache, 500),
        ("Sampel latihan kelelahan", s.sampel, 1000),
        ("Lagu favorit tercatat", s.lagu, 50),
    ]
}

/// Persen 0..100 rata-rata kemajuan, dibatasi 100 per dimensi.
pub fn tingkat(s: &Snap) -> i64 {
    let d = dimensi(s);
    (d.iter().map(|(_, v, t)| (*v as f64 / *t as f64).min(1.0)).sum::<f64>() / d.len() as f64 * 100.0).round() as i64
}

fn hitung(sql: &str, c: &rusqlite::Connection) -> i64 {
    c.query_row(sql, [], |r| r.get(0)).unwrap_or(0)
}

pub fn sekarang() -> Snap {
    let mut s = Snap { hari: chrono::Local::now().format("%Y-%m-%d").to_string(), fakta: crate::fakta::jumlah(), cache: crate::gemini::cache_stats().0, ..Default::default() };
    if let Ok(c) = crate::teknik::open() {
        s.contoh = hitung("SELECT COUNT(*) FROM intent_ex", &c);
        s.pengetahuan = hitung("SELECT COUNT(*) FROM kb", &c);
    }
    (s.graf_simpul, s.graf_sisi) = crate::graf::hitung();
    let (n, acc, _, _) = crate::belajar::stats();
    s.sampel = n as i64;
    s.akurasi = acc.round() as i64;
    s.lagu = crate::musik::jumlah_lagu();
    s
}

fn log_file() -> std::path::PathBuf {
    crate::config::otak_dir().join("otak_log.json")
}

pub fn riwayat() -> Vec<Snap> {
    std::fs::read_to_string(log_file()).ok().and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default()
}

/// Simpan snapshot hari ini (menimpa yang sama), simpan 90 hari terakhir.
fn simpan(s: &Snap) {
    let mut v = riwayat();
    v.retain(|x| x.hari != s.hari);
    v.push(s.clone());
    let n = v.len().saturating_sub(90);
    v.drain(..n);
    let _ = std::fs::write(log_file(), serde_json::to_string(&v).unwrap_or_default());
}

/// Belajar berkelanjutan: melatih ulang model kelelahan dari data terbaru lalu mencatat pertumbuhan. Sekali per hari saat Bos aktif.
pub fn harian(paksa: bool) -> Option<String> {
    if !paksa && !crate::assistant::once_key(&format!("{}otak", chrono::Local::now().format("%Y-%m-%d"))) {
        return None;
    }
    let r = crate::belajar::retrain();
    let s = sekarang();
    simpan(&s);
    Some(format!("{r} · tingkat otak {}%", tingkat(&s)))
}

pub fn json() -> serde_json::Value {
    let s = sekarang();
    let d: Vec<serde_json::Value> = dimensi(&s).into_iter().map(|(n, v, t)| serde_json::json!({"nama": n, "nilai": v, "target": t, "persen": ((v as f64 / t as f64).min(1.0) * 100.0).round()})).collect();
    serde_json::json!({"tingkat": tingkat(&s), "dimensi": d, "akurasi": s.akurasi, "graf_sisi": s.graf_sisi, "riwayat": riwayat().iter().map(|x| serde_json::json!({"hari": x.hari, "tingkat": tingkat(x)})).collect::<Vec<_>>()})
}

pub fn teks() -> String {
    let s = sekarang();
    let mut t = format!("🧠 Tingkat otak NOVA: {}%\n", tingkat(&s));
    for (n, v, tg) in dimensi(&s) {
        let p = ((v as f64 / tg as f64).min(1.0) * 10.0).round() as usize;
        t.push_str(&format!("{:<32} {}{} {}/{}\n", n, "█".repeat(p), "░".repeat(10 - p), v, tg));
    }
    if let Some(o) = crate::data::json("eskalasi.json", "{}").as_object() {
        let (p, n) = o.values().fold((0, 0), |a, v| (a.0 + v[0].as_u64().unwrap_or(0), a.1 + v[1].as_u64().unwrap_or(0)));
        if p + n > 0 {
            t.push_str(&format!("Pertanyaan pengetahuan: {p} dijawab lokal, {n} naik ke model ({}% perlu model).
", n * 100 / (p + n)));
        }
    }
    t.push_str(&format!("Akurasi prediksi kelelahan: {}% · sisi graf: {}\nBelajar berkelanjutan: tiap hari saat Bos aktif NOVA melatih ulang dan mencatat pertumbuhan; kini {} hari tercatat.", s.akurasi, s.graf_sisi, riwayat().len()));
    t
}

/// `/otak pindah [folder]`: salin berkas otak ke `folder` (bawaan: folder `otak` di sebelah repo/kode) dan jadikan lokasi aktif.
/// Mulai ulang NOVA setelahnya agar semua bagian memakai lokasi baru; salinan lama di folder data tidak dihapus.
pub fn pindah(tujuan: &str) -> String {
    let t = if tujuan.is_empty() {
        let exe = std::env::current_exe().unwrap_or_default();
        match exe.ancestors().find(|p| p.join("Cargo.toml").exists()) {
            Some(repo) => repo.join("otak"),
            None => return "Sebut folder tujuannya: /otak pindah D:/repo/rustgent/otak".into(),
        }
    } else {
        std::path::PathBuf::from(tujuan.trim_matches('"'))
    };
    if std::fs::create_dir_all(&t).is_err() {
        return format!("Tidak bisa membuat {}.", t.display());
    }
    let (asal, mut n) = (crate::config::otak_dir(), 0);
    for f in crate::config::BERKAS_OTAK {
        let (a, b) = (asal.join(f), t.join(f));
        if a.exists() && a != b && std::fs::copy(&a, &b).is_ok() {
            n += 1;
        }
    }
    let _ = std::fs::write(crate::config::data_dir().join("otak_path.txt"), t.display().to_string());
    format!("{n} berkas otak disalin ke {} dan dijadikan lokasi otak. Mulai ulang NOVA agar semua bagian memakainya.", t.display())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn level_is_capped_average() {
        assert_eq!(tingkat(&Snap::default()), 0);
        let s = Snap { fakta: 1000, contoh: 1000, pengetahuan: 9999, graf_simpul: 9999, cache: 9999, sampel: 9999, lagu: 999, ..Default::default() };
        assert_eq!(tingkat(&s), 100);
        let h = Snap { fakta: 50, ..Default::default() };
        assert_eq!(tingkat(&h), 7); // 0,5 / 7 dimensi
    }
}
