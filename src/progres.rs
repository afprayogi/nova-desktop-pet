//! Indikator kemajuan bergaya robot: pekerjaan apa pun (misi, pembuatan keahlian, pencarian web, ...) melapor lewat sini dan
//! antarmuka menampilkan bar persen + langkah + waktu. Global dan aman antar-thread, jadi modul mana pun bisa melapor tanpa `Tx`.
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

struct Job {
    id: u64,
    judul: String,
    label: String,
    pct: u8,
    gagal: bool,
    mulai: Instant,
    akhir: Option<Instant>,
}

static JOBS: Mutex<Vec<Job>> = Mutex::new(Vec::new());
static NEXT: AtomicU64 = AtomicU64::new(1);

#[derive(Serialize, Clone, Default)]
pub struct JobDto {
    pub id: u64,
    pub judul: String,
    pub label: String,
    pub pct: u8,
    pub selesai: bool,
    pub gagal: bool,
    pub detik: u64,
}

fn kunci() -> std::sync::MutexGuard<'static, Vec<Job>> {
    JOBS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Mulai pekerjaan baru (0%). Kembalikan id untuk `set`/`selesai`.
pub fn mulai(judul: &str, label: &str) -> u64 {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let mut j = kunci();
    j.push(Job { id, judul: judul.chars().take(60).collect(), label: label.chars().take(80).collect(), pct: 0, gagal: false, mulai: Instant::now(), akhir: None });
    if j.len() > 8 {
        j.remove(0);
    }
    id
}

/// Atur persen (0..=99 selama berjalan) dan keterangan langkah saat ini.
pub fn set(id: u64, pct: u8, label: &str) {
    if let Some(x) = kunci().iter_mut().find(|x| x.id == id && x.akhir.is_none()) {
        x.pct = pct.min(99).max(x.pct.min(99));
        x.label = label.chars().take(80).collect();
    }
}

/// Persen dari langkah `k` dari `n` (k mulai 1: langkah yang sedang dikerjakan belum dihitung selesai).
pub fn langkah(id: u64, k: usize, n: usize, label: &str) {
    set(id, (k.saturating_sub(1) * 100 / n.max(1)) as u8, &format!("langkah {k}/{n} · {label}"));
}

pub fn selesai(id: u64, label: &str) {
    if let Some(x) = kunci().iter_mut().find(|x| x.id == id) {
        x.pct = 100;
        x.label = label.chars().take(80).collect();
        x.akhir = Some(Instant::now());
    }
}

pub fn gagal(id: u64, label: &str) {
    if let Some(x) = kunci().iter_mut().find(|x| x.id == id) {
        x.gagal = true;
        x.label = label.chars().take(80).collect();
        x.akhir = Some(Instant::now());
    }
}

/// Yang berjalan + yang baru selesai (tetap terlihat 6 detik agar "100%" sempat terbaca).
pub fn snapshot() -> Vec<JobDto> {
    let mut j = kunci();
    j.retain(|x| x.akhir.is_none_or(|t| t.elapsed().as_secs() < 6));
    j.iter()
        .map(|x| JobDto { id: x.id, judul: x.judul.clone(), label: x.label.clone(), pct: x.pct, selesai: x.akhir.is_some() && !x.gagal, gagal: x.gagal, detik: x.akhir.unwrap_or_else(Instant::now).duration_since(x.mulai).as_secs() })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn persen_naik_dan_selesai() {
        let id = super::mulai("uji", "mulai");
        super::langkah(id, 1, 4, "a");
        super::langkah(id, 3, 4, "b");
        let s = super::snapshot().into_iter().find(|x| x.id == id).unwrap();
        assert_eq!(s.pct, 50);
        super::set(id, 10, "mundur tidak boleh");
        assert_eq!(super::snapshot().into_iter().find(|x| x.id == id).unwrap().pct, 50);
        super::selesai(id, "beres");
        let s = super::snapshot().into_iter().find(|x| x.id == id).unwrap();
        assert!(s.selesai && s.pct == 100);
    }
}
