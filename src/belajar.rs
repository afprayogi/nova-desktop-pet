//! Deep learning kecil (MLP 4-8-1, ditulis sendiri tanpa pustaka) untuk memprediksi "Bos akan berhenti/istirahat dalam 10
//! menit ke depan" dari panjang sesi aktif dan jam. Dilatih dari riwayat aktivitas per menit (tabel `act`) saat mulai dan
//! setiap hari; bobot disimpan lokal. Hasilnya dipadukan dengan aturan pada skor kelelahan, jadi makin lama dipakai makin akurat.
use serde::{Deserialize, Serialize};
use std::sync::RwLock;

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Net {
    w1: Vec<[f32; 4]>, // 8 neuron x 4 fitur
    b1: Vec<f32>,
    w2: Vec<f32>,
    b2: f32,
    pub samples: usize,
    pub accuracy: f32,
    pub baseline: f32,
    pub trained_at: String,
}

static NET: RwLock<Option<Net>> = RwLock::new(None);

fn path() -> std::path::PathBuf {
    crate::config::data_dir().join("belajar.json")
}

fn feats(streak: u32, hour: u32) -> [f32; 4] {
    let a = std::f32::consts::TAU * hour as f32 / 24.0;
    [(streak as f32 / 120.0).min(2.0), a.sin(), a.cos(), (hour >= 23 || hour < 5) as u8 as f32]
}

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

impl Net {
    fn init() -> Net {
        let mut s = 0x9E3779B9u32;
        let mut r = || {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            (s as f32 / u32::MAX as f32 - 0.5) * 0.8
        };
        Net { w1: (0..8).map(|_| [r(), r(), r(), r()]).collect(), b1: vec![0.0; 8], w2: (0..8).map(|_| r()).collect(), b2: 0.0, ..Default::default() }
    }

    fn forward(&self, x: &[f32; 4]) -> ([f32; 8], f32) {
        let mut h = [0f32; 8];
        for (i, hi) in h.iter_mut().enumerate() {
            let z: f32 = self.w1[i].iter().zip(x).map(|(a, b)| a * b).sum::<f32>() + self.b1[i];
            *hi = z.tanh();
        }
        (h, sigmoid(h.iter().zip(&self.w2).map(|(a, b)| a * b).sum::<f32>() + self.b2))
    }

    pub fn predict(&self, streak: u32, hour: u32) -> f32 {
        self.forward(&feats(streak, hour)).1
    }

    fn train(&mut self, data: &[([f32; 4], f32)], epochs: usize, lr: f32) {
        for e in 0..epochs {
            let lr = lr / (1.0 + e as f32 * 0.05);
            for (x, y) in data {
                let (h, p) = self.forward(x);
                let d = p - y; // dL/dz untuk cross-entropy + sigmoid
                for i in 0..8 {
                    let dh = d * self.w2[i] * (1.0 - h[i] * h[i]);
                    self.w2[i] -= lr * d * h[i];
                    for k in 0..4 {
                        self.w1[i][k] -= lr * dh * x[k];
                    }
                    self.b1[i] -= lr * dh;
                }
                self.b2 -= lr * d;
            }
        }
    }
}

/// Sampel dari daftar menit aktif (terurut): fitur (panjang sesi, jam) dan label "sesi berakhir dalam 10 menit".
fn build_samples(mins: &[i64], now_min: i64, hour_of: impl Fn(i64) -> u32) -> Vec<([f32; 4], f32)> {
    let mut out = vec![];
    let mut start = 0usize;
    for i in 0..mins.len() {
        if i > 0 && mins[i] - mins[i - 1] > 3 {
            start = i;
        }
        if now_min - mins[i] < 12 {
            continue; // masa depan belum diketahui
        }
        let streak = (mins[i] - mins[start]) as u32;
        let ends = mins.get(i + 1).map_or(true, |n| n - mins[i] > 10);
        out.push((feats(streak, hour_of(mins[i])), ends as u8 as f32));
    }
    out
}

/// Latih ulang dari riwayat (14 hari). Mengembalikan ringkasan.
pub fn retrain() -> String {
    use chrono::{TimeZone, Timelike};
    let Ok(c) = crate::assistant::db() else { return "database tidak tersedia".into() };
    let now_min = chrono::Utc::now().timestamp() / 60;
    let mins: Vec<i64> = c.prepare("SELECT min FROM act WHERE min >= ?1 ORDER BY min").ok().and_then(|mut s| s.query_map([now_min - 14 * 1440], |r| r.get(0)).ok().map(|r| r.flatten().collect())).unwrap_or_default();
    let mut data = build_samples(&mins, now_min, |m| chrono::Local.timestamp_opt(m * 60, 0).single().map(|d| d.hour()).unwrap_or(12));
    if data.len() < 100 {
        return format!("Data belum cukup untuk melatih ({} sampel, butuh ≥100).", data.len());
    }
    if data.len() > 6000 {
        let step = data.len() / 6000 + 1;
        data = data.into_iter().step_by(step).collect();
    }
    let mut net = Net::init();
    net.train(&data, 40, 0.05);
    let correct = data.iter().filter(|(x, y)| (net.forward(x).1 > 0.5) == (*y > 0.5)).count();
    let pos = data.iter().filter(|(_, y)| *y > 0.5).count();
    net.samples = data.len();
    net.accuracy = correct as f32 / data.len() as f32;
    net.baseline = (pos.max(data.len() - pos)) as f32 / data.len() as f32;
    net.trained_at = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
    let msg = format!("Jaringan saraf dilatih: {} sampel, akurasi {:.0}% (tebakan mayoritas {:.0}%).", net.samples, net.accuracy * 100.0, net.baseline * 100.0);
    let _ = std::fs::write(path(), serde_json::to_string(&net).unwrap_or_default());
    if let Ok(mut g) = NET.write() {
        *g = Some(net);
    }
    msg
}

fn load() -> Option<Net> {
    if let Ok(g) = NET.read() {
        if g.is_some() {
            return g.clone();
        }
    }
    let n: Net = serde_json::from_str(&std::fs::read_to_string(path()).ok()?).ok()?;
    if let Ok(mut g) = NET.write() {
        *g = Some(n.clone());
    }
    Some(n)
}

/// Peluang Bos berhenti dalam 10 menit (None bila model belum cukup baik: harus mengalahkan tebakan mayoritas).
pub fn p_break(streak: u32) -> Option<f32> {
    use chrono::Timelike;
    let n = load()?;
    (n.samples >= 300 && n.accuracy > n.baseline + 0.01).then(|| n.predict(streak, chrono::Local::now().hour()))
}

pub fn info() -> String {
    match load() {
        Some(n) => format!("Model ML: MLP 4-8-1, {} sampel, akurasi {:.0}% vs mayoritas {:.0}% (dilatih {}). {}", n.samples, n.accuracy * 100.0, n.baseline * 100.0, n.trained_at, if n.accuracy > n.baseline + 0.01 { "Dipakai untuk prediksi." } else { "Belum lebih baik dari tebakan mayoritas; belum dipakai." }),
        None => "Model ML: belum dilatih (butuh ≥100 menit aktivitas; otomatis tiap hari, atau /latih).".into(),
    }
}

/// Untuk dasbor: (sampel, akurasi %, mayoritas %, dipakai?)
pub fn stats() -> (usize, f32, f32, bool) {
    load().map(|n| (n.samples, n.accuracy * 100.0, n.baseline * 100.0, n.accuracy > n.baseline + 0.01)).unwrap_or((0, 0.0, 0.0, false))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn learns_that_long_sessions_end() {
        // sesi sintetis: aktif 90 menit lalu istirahat 30 menit, berulang
        let mut mins = vec![];
        let mut t = 1000;
        for _ in 0..12 {
            mins.extend(t..t + 90);
            t += 90 + 30;
        }
        let data = build_samples(&mins, t + 100, |_| 10);
        assert!(data.len() > 500);
        let mut n = Net::init();
        n.train(&data, 60, 0.05);
        assert!(n.predict(88, 10) > n.predict(10, 10) + 0.2, "{} vs {}", n.predict(88, 10), n.predict(10, 10));
    }
}
