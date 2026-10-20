//! Syarat per langkah untuk rantai otomasi: "lakukan B hanya jika A berhasil", "kecuali sedang mengisi daya", dst.
//! Kosakata kecil dan tetap (tidak ada eval): beberapa syarat digabung dengan " dan "; awalan "!" atau "bukan " membalik (kecuali).
//!   sebelumnya_ok | sebelumnya_gagal | ac | baterai<N | baterai>N | jam>=HH:MM | jam<HH:MM | hari:senin,jumat | app:nama | wifi:nama | kerja | ada:<path>
use chrono::{Datelike, Local, NaiveTime};

/// Dimasukkan ke prompt perancang jadwal/pemicu agar model memakai kosakata syarat yang benar.
pub const PETUNJUK_MODEL: &str = "Tiap langkah boleh punya kolom opsional \"jika\" berisi syarat (gabung dengan ' dan ', awali '!' untuk 'kecuali'): sebelumnya_ok | sebelumnya_gagal | ac | baterai<N | baterai>N | jam>=HH:MM | jam<HH:MM | hari:senin,jumat | app:nama | wifi:nama | kerja | ada:path. Contoh: \"jika\":\"!ac dan baterai<30\".";

fn hari(s: &str) -> Option<u32> {
    ["senin", "selasa", "rabu", "kamis", "jumat", "sabtu", "minggu"].iter().position(|x| *x == s.trim().to_lowercase()).map(|i| i as u32)
}

fn jam(s: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(s.trim(), "%H:%M").ok()
}

/// Satu syarat tunggal (tanpa "dan"/negasi) harus sah menurut kosakata; dipakai saat validasi rancangan.
fn sah_tunggal(t: &str) -> bool {
    let t = t.trim();
    match t {
        "sebelumnya_ok" | "sebelumnya_gagal" | "ac" | "kerja" => true,
        _ => {
            if let Some(n) = t.strip_prefix("baterai<").or_else(|| t.strip_prefix("baterai>")) {
                n.trim().parse::<u32>().is_ok_and(|n| n <= 100)
            } else if let Some(j) = t.strip_prefix("jam>=").or_else(|| t.strip_prefix("jam<")) {
                jam(j).is_some()
            } else if let Some(h) = t.strip_prefix("hari:") {
                !h.is_empty() && h.split(',').all(|x| hari(x).is_some())
            } else if let Some(a) = t.strip_prefix("app:").or_else(|| t.strip_prefix("wifi:")) {
                (2..=40).contains(&a.trim().chars().count())
            } else if let Some(p) = t.strip_prefix("ada:") {
                (2..=200).contains(&p.chars().count()) && !p.contains(['\n', '\r', '*', '?'])
            } else {
                false
            }
        }
    }
}

fn pisah(expr: &str) -> Vec<(bool, String)> {
    expr.split(" dan ").map(|p| {
        let p = p.trim();
        match p.strip_prefix('!').or_else(|| p.strip_prefix("bukan ")) {
            Some(r) => (true, r.trim().to_string()),
            None => (false, p.to_string()),
        }
    }).collect()
}

pub fn validasi(expr: &str) -> Result<(), String> {
    if expr.chars().count() > 120 || expr.contains(['\n', '\r']) {
        return Err("syarat terlalu panjang".into());
    }
    for (_, t) in pisah(expr) {
        if !sah_tunggal(&t) {
            return Err(format!("syarat tidak dikenal: '{t}'"));
        }
    }
    Ok(())
}

fn tunggal(t: &str, sebelumnya_ok: bool, sekarang: chrono::DateTime<Local>) -> bool {
    match t {
        "sebelumnya_ok" => sebelumnya_ok,
        "sebelumnya_gagal" => !sebelumnya_ok,
        "ac" => crate::baterai::read().is_some_and(|b| b.ac),
        "kerja" => crate::kerja::dalam_jam_kerja(),
        _ => {
            if let Some(n) = t.strip_prefix("baterai<") {
                crate::baterai::read().is_some_and(|b| n.trim().parse::<f32>().is_ok_and(|n| b.pct < n))
            } else if let Some(n) = t.strip_prefix("baterai>") {
                crate::baterai::read().is_some_and(|b| n.trim().parse::<f32>().is_ok_and(|n| b.pct > n))
            } else if let Some(j) = t.strip_prefix("jam>=") {
                jam(j).is_some_and(|j| sekarang.time() >= j)
            } else if let Some(j) = t.strip_prefix("jam<") {
                jam(j).is_some_and(|j| sekarang.time() < j)
            } else if let Some(h) = t.strip_prefix("hari:") {
                h.split(',').any(|x| hari(x) == Some(sekarang.weekday().num_days_from_monday()))
            } else if let Some(a) = t.strip_prefix("app:") {
                crate::konteks::current_app().to_lowercase().contains(&a.trim().to_lowercase())
            } else if let Some(w) = t.strip_prefix("wifi:") {
                crate::pemicu::ssid().is_some_and(|s| s.to_lowercase().contains(&w.trim().to_lowercase()))
            } else if let Some(p) = t.strip_prefix("ada:") {
                std::path::Path::new(p.trim()).exists()
            } else {
                false
            }
        }
    }
}

/// Apakah langkah dengan syarat `expr` boleh jalan sekarang? Syarat tak dikenal dianggap tidak terpenuhi (aman).
pub fn terpenuhi(expr: &str, sebelumnya_ok: bool) -> bool {
    terpenuhi_pada(expr, sebelumnya_ok, Local::now())
}

pub fn terpenuhi_pada(expr: &str, sebelumnya_ok: bool, sekarang: chrono::DateTime<Local>) -> bool {
    pisah(expr).iter().all(|(balik, t)| tunggal(t, sebelumnya_ok, sekarang) != *balik)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn validasi_kosakata() {
        for ok in ["sebelumnya_ok", "!ac", "bukan ac dan baterai<30", "jam>=08:00 dan jam<16:30", "hari:senin,jumat", "app:code", "ada:C:/Users/a.txt"] {
            assert!(validasi(ok).is_ok(), "{ok}");
        }
        for salah in ["", "rm -rf /", "baterai<abc", "jam>=25:00", "hari:funday", "ada:*", "sebelumnya_ok; calc"] {
            assert!(validasi(salah).is_err(), "{salah}");
        }
    }

    #[test]
    fn evaluasi_waktu_rantai_dan_kecuali() {
        let selasa_9 = Local.with_ymd_and_hms(2026, 10, 20, 9, 0, 0).unwrap();
        assert!(terpenuhi_pada("jam>=08:00 dan jam<16:30", true, selasa_9));
        assert!(!terpenuhi_pada("jam>=10:00", true, selasa_9));
        assert!(terpenuhi_pada("hari:selasa", true, selasa_9) && !terpenuhi_pada("hari:senin,jumat", true, selasa_9));
        assert!(terpenuhi_pada("!hari:sabtu,minggu", true, selasa_9)); // kecuali akhir pekan
        assert!(terpenuhi_pada("sebelumnya_ok", true, selasa_9) && !terpenuhi_pada("sebelumnya_ok", false, selasa_9));
        assert!(terpenuhi_pada("sebelumnya_gagal", false, selasa_9));
        assert!(!terpenuhi_pada("ada:Z:/tidak/ada", true, selasa_9));
    }
}
