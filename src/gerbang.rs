//! Gerbang perhatian + penjaga beban.
//! 1) Semua notifikasi OS lewat sini: yang kritis selalu tampil; sisanya bisa ditunda (jam tenang, mode fokus, kecerewetan 0, kuota harian)
//!    dan dikirim sebagai satu ringkasan saat Bos sedang jeda. 2) `longgar()` menahan pekerjaan latar yang berat sampai komputer lengang,
//!    supaya NOVA tidak berebut CPU/RAM dengan pekerjaan Bos di komputer lemah.
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Pemakaian CPU (%) dan RAM (%) terakhir, diisi pemantau; 0 bila belum ada data.
pub static CPU: AtomicU32 = AtomicU32::new(0);
pub static RAM: AtomicU32 = AtomicU32::new(0);

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tingkat {
    Kritis,
    Penting,
    Biasa,
    Rendah,
}

#[derive(PartialEq, Debug)]
pub enum Putusan {
    Tampil,
    Tunda,
    Diam,
}

pub fn tingkat(judul: &str) -> Tingkat {
    let t = judul.to_lowercase();
    let ada = |ks: &[&str]| ks.iter().any(|k| t.contains(k));
    if ada(&["butuh izin", "baterai", "peringatan", "nova aktif"]) {
        Tingkat::Kritis
    } else if ada(&["berita", "harga", "cuaca", "naik level", "pencapaian", "buku dipelajari", "build selesai", "pantauan"]) {
        Tingkat::Rendah
    } else if ada(&["solusi error", "kode", "anomali", "email baru", "saran", "otomatis", "fokus selesai", "build gagal"]) {
        Tingkat::Biasa
    } else {
        Tingkat::Penting
    }
}

/// Aturan murni (mudah diuji): tingkat x keadaan -> putusan.
pub fn nilai(t: Tingkat, fokus: bool, malam: bool, cerewet: u8, kuota_ok: bool) -> Putusan {
    match t {
        Tingkat::Kritis => Putusan::Tampil,
        Tingkat::Penting if fokus => Putusan::Tunda,
        Tingkat::Penting => Putusan::Tampil,
        Tingkat::Rendah if cerewet == 0 => Putusan::Diam,
        Tingkat::Biasa | Tingkat::Rendah if fokus || malam || cerewet == 0 || !kuota_ok => Putusan::Tunda,
        _ => Putusan::Tampil,
    }
}

struct Hitung {
    hari: i64,
    biasa: u32,
    rendah: u32,
    terakhir: Option<Instant>,
}

static HITUNG: Mutex<Hitung> = Mutex::new(Hitung { hari: 0, biasa: 0, rendah: 0, terakhir: None });
static ANTRE: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());
static RINGKAS: Mutex<Option<Instant>> = Mutex::new(None);

/// Jam tenang: 30 menit sebelum jam tidur sampai jam bangun (dari jadwal.json).
fn malam() -> bool {
    let (bed, wake) = crate::assistant::sleep_window();
    let m = crate::assistant::day_min();
    let mulai = (bed - 30).rem_euclid(1440);
    if mulai > wake { m >= mulai || m < wake } else { m >= mulai && m < wake }
}

/// Kuota harian dan jarak antar pesan non-penting; menghitung bila diizinkan.
fn kuota(t: Tingkat) -> bool {
    if matches!(t, Tingkat::Kritis | Tingkat::Penting) {
        return true;
    }
    let mut h = HITUNG.lock().unwrap_or_else(|e| e.into_inner());
    let hari = chrono::Local::now().timestamp() / 86_400;
    if h.hari != hari {
        *h = Hitung { hari, biasa: 0, rendah: 0, terakhir: None };
    }
    let (n, maks, jeda) = if t == Tingkat::Biasa { (h.biasa, 6, 10) } else { (h.rendah, 3, 30) };
    if n >= maks || h.terakhir.is_some_and(|x| x.elapsed() < Duration::from_secs(jeda * 60)) {
        return false;
    }
    if t == Tingkat::Biasa { h.biasa += 1 } else { h.rendah += 1 }
    h.terakhir = Some(Instant::now());
    true
}

fn senyap_file() -> std::path::PathBuf {
    crate::config::data_dir().join("senyap_on")
}

/// Mode senyap: peliharaan disembunyikan dan hanya pesan kritis (izin, baterai, peringatan sistem) yang tampil.
pub fn senyap() -> bool {
    senyap_file().exists()
}

pub fn set_senyap(on: bool) {
    if on {
        let _ = std::fs::write(senyap_file(), "");
    } else {
        let _ = std::fs::remove_file(senyap_file());
    }
}

pub fn putuskan(judul: &str) -> Putusan {
    let t = tingkat(judul);
    if t == Tingkat::Kritis {
        return Putusan::Tampil;
    }
    let (fokus, malam, cerewet) = (crate::bandit::quiet() || senyap(), malam(), crate::adegan::level());
    // kuota dicek terakhir agar tidak terhitung untuk pesan yang toh ditunda.
    let tampil_dulu = nilai(t, fokus, malam, cerewet, true);
    if tampil_dulu == Putusan::Tampil && !kuota(t) {
        return Putusan::Tunda;
    }
    tampil_dulu
}

pub fn simpan(judul: &str, isi: &str) {
    let mut q = ANTRE.lock().unwrap_or_else(|e| e.into_inner());
    q.retain(|(j, _)| j != judul);
    if q.len() >= 20 {
        q.remove(0);
    }
    q.push((judul.to_string(), isi.to_string()));
}

pub fn tertunda() -> usize {
    ANTRE.lock().map(|q| q.len()).unwrap_or(0)
}

/// Dipanggil berkala: kirim pesan tertunda sebagai satu ringkasan saat Bos jeda (idle >= 45 dtk), bukan fokus/malam.
pub fn siram() {
    if tertunda() == 0 || crate::bandit::quiet() || senyap() || malam() || crate::konteks::IDLE_SECS.load(Ordering::Relaxed) < 45 {
        return;
    }
    {
        let mut r = RINGKAS.lock().unwrap_or_else(|e| e.into_inner());
        if r.is_some_and(|x| x.elapsed() < Duration::from_secs(20 * 60)) {
            return;
        }
        *r = Some(Instant::now());
    }
    let q: Vec<(String, String)> = std::mem::take(&mut *ANTRE.lock().unwrap_or_else(|e| e.into_inner()));
    match q.as_slice() {
        [] => {}
        [(j, i)] => crate::notify::kirim(j, i),
        _ => {
            let isi = q.iter().take(4).map(|(j, i)| format!("{}: {}", j.trim_start_matches("NOVA - "), i.lines().next().unwrap_or(""))).collect::<Vec<_>>().join("\n");
            crate::notify::kirim(&format!("NOVA - {} pesan tertunda", q.len()), &isi);
        }
    }
}

/// Boleh menjalankan pekerjaan latar yang berat sekarang? (Bos jeda >= 60 dtk atau malam, CPU < 50%, RAM < 85%)
pub fn longgar() -> bool {
    let idle = crate::konteks::IDLE_SECS.load(Ordering::Relaxed);
    (idle >= 60 || malam()) && CPU.load(Ordering::Relaxed) < 50 && RAM.load(Ordering::Relaxed) < 85
}

/// Tunggu sampai komputer lengang (cek tiap 30 dtk, paling lama 3 jam agar belajar latar tidak mati bila Bos selalu aktif).
pub async fn tunggu_longgar() {
    for _ in 0..360 {
        if longgar() {
            break;
        }
        tokio::time::sleep(Duration::from_secs(30)).await;
    }
}

/// Versi untuk thread biasa.
pub fn tunggu_longgar_sync() {
    for _ in 0..360 {
        if longgar() {
            break;
        }
        std::thread::sleep(Duration::from_secs(30));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Putusan::*;
    use Tingkat::*;

    #[test]
    fn judul_dipetakan_ke_tingkat() {
        assert_eq!(tingkat("NOVA butuh izin"), Kritis);
        assert_eq!(tingkat("NOVA - baterai"), Kritis);
        assert_eq!(tingkat("NOVA - berita sesuai minat"), Rendah);
        assert_eq!(tingkat("NOVA - email baru"), Biasa);
        assert_eq!(tingkat("NOVA"), Penting);
    }

    #[test]
    fn mode_senyap_menahan_semua_kecuali_kritis() {
        set_senyap(true);
        assert!(senyap());
        assert_eq!(putuskan("NOVA butuh izin"), Putusan::Tampil); // kritis tetap tampil
        assert_ne!(putuskan("NOVA - email baru"), Putusan::Tampil);
        assert_ne!(putuskan("NOVA"), Putusan::Tampil); // pengingat biasa ditahan
        set_senyap(false);
        assert!(!senyap());
    }

    #[test]
    fn aturan_gerbang() {
        assert_eq!(nilai(Kritis, true, true, 0, false), Tampil);
        assert_eq!(nilai(Penting, false, true, 1, true), Tampil); // pengingat tidur tetap tampil saat jam tenang
        assert_eq!(nilai(Penting, true, false, 1, true), Tunda); // fokus menahan
        assert_eq!(nilai(Biasa, false, true, 1, true), Tunda); // malam menahan yang biasa
        assert_eq!(nilai(Rendah, false, false, 0, true), Diam); // cerewet 0: yang remeh dibuang
        assert_eq!(nilai(Rendah, false, false, 1, false), Tunda); // kuota habis
        assert_eq!(nilai(Biasa, false, false, 1, true), Tampil);
    }
}
