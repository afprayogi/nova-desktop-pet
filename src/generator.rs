//! Generator paket karakter: dari SATU gambar (PNG/JPG berlatar transparan paling bagus) membuat satu set bingkai gaya Shimeji
//! (walk/drag/fall/climb/idle/special) lewat transformasi afin (goyang, regang, putar, lompat, berputar), lalu menyusun metadata.
//! Juga membuat efek suara (naik level, pencapaian, error, blip) secara sintetis sebagai berkas WAV. Terinspirasi fitur
//! "custom pet package generator" pada proyek agentpet.
use image::{imageops, Rgba, RgbaImage};
use std::path::PathBuf;

const N: u32 = 128;

pub fn dir_suara() -> PathBuf {
    let d = crate::config::data_dir().join("suara");
    let _ = std::fs::create_dir_all(&d);
    d
}

// ---------------- efek suara ----------------

fn tone(out: &mut Vec<i16>, hz: f32, sec: f32, saw: bool, vol: f32) {
    let sr = 22050.0;
    let n = (sr * sec) as usize;
    for i in 0..n {
        let t = i as f32 / sr;
        let ph = (hz * t).fract();
        let w = if saw { ph * 2.0 - 1.0 } else { (std::f32::consts::TAU * hz * t).sin() };
        let env = (1.0 - i as f32 / n as f32).powf(1.4).min(i as f32 / 120.0 + 0.001).min(1.0);
        out.push((w * env * vol * 30000.0) as i16);
    }
}

fn write_wav(path: &PathBuf, s: &[i16]) {
    let data: Vec<u8> = s.iter().flat_map(|v| v.to_le_bytes()).collect();
    let mut w = Vec::with_capacity(44 + data.len());
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&22050u32.to_le_bytes());
    w.extend_from_slice(&(22050u32 * 2).to_le_bytes());
    w.extend_from_slice(&2u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&(data.len() as u32).to_le_bytes());
    w.extend_from_slice(&data);
    let _ = std::fs::write(path, w);
}

/// Buat set efek suara bila belum ada. Mengembalikan jumlah berkas yang dibuat.
pub fn bikin_suara() -> usize {
    let d = dir_suara();
    let mut n = 0;
    let sets: Vec<(&str, Vec<(f32, f32, bool)>)> = vec![
        ("levelup", vec![(523.25, 0.09, false), (659.25, 0.09, false), (783.99, 0.09, false), (1046.5, 0.22, false)]),
        ("achievement", vec![(783.99, 0.1, false), (1046.5, 0.1, false), (1318.5, 0.28, false)]),
        ("error", vec![(196.0, 0.16, true), (146.8, 0.26, true)]),
        ("blip", vec![(880.0, 0.06, false)]),
    ];
    for (name, notes) in sets {
        let p = d.join(format!("{name}.wav"));
        if p.exists() {
            continue;
        }
        let mut s = vec![];
        for (hz, sec, saw) in notes {
            tone(&mut s, hz, sec, saw, 0.35);
        }
        write_wav(&p, &s);
        n += 1;
    }
    n
}

// ---------------- bingkai sprite ----------------

/// Gambar `src` (128×128, jangkar kaki di (64,124)) setelah: skala (sx, sy), putar derajat, geser (dx, dy).
fn xform(src: &RgbaImage, sx: f32, sy: f32, rot: f32, dx: f32, dy: f32) -> RgbaImage {
    let mut out = RgbaImage::new(N, N);
    let (ax, ay) = (64.0f32, 124.0f32);
    let (c, s) = ((-rot.to_radians()).cos(), (-rot.to_radians()).sin());
    for y in 0..N {
        for x in 0..N {
            let (px, py) = (x as f32 - ax - dx, y as f32 - ay - dy);
            let (rx, ry) = (px * c - py * s, px * s + py * c);
            let (u, v) = (rx / sx.max(0.05) + ax, ry / sy.max(0.05) + ay);
            if u < 0.0 || v < 0.0 || u >= (N - 1) as f32 || v >= (N - 1) as f32 {
                continue;
            }
            let (x0, y0) = (u.floor() as u32, v.floor() as u32);
            let (fx, fy) = (u - x0 as f32, v - y0 as f32);
            let g = |xx: u32, yy: u32| src.get_pixel(xx, yy).0;
            let (p00, p10, p01, p11) = (g(x0, y0), g(x0 + 1, y0), g(x0, y0 + 1), g(x0 + 1, y0 + 1));
            let mut px_out = [0u8; 4];
            for k in 0..4 {
                let top = p00[k] as f32 * (1.0 - fx) + p10[k] as f32 * fx;
                let bot = p01[k] as f32 * (1.0 - fx) + p11[k] as f32 * fx;
                px_out[k] = (top * (1.0 - fy) + bot * fy) as u8;
            }
            out.put_pixel(x, y, Rgba(px_out));
        }
    }
    out
}

fn siapkan(path: &str) -> Result<RgbaImage, String> {
    let img = image::open(path).map_err(|e| format!("tidak bisa membuka gambar: {e}"))?.to_rgba8();
    let (w, h) = (img.width() as f32, img.height() as f32);
    let k = (104.0 / w).min(104.0 / h);
    let (nw, nh) = (((w * k) as u32).max(1), ((h * k) as u32).max(1));
    let small = imageops::resize(&img, nw, nh, imageops::FilterType::Lanczos3);
    let mut base = RgbaImage::new(N, N);
    imageops::overlay(&mut base, &small, ((N - nw) / 2) as i64, (124 - nh as i32).max(0) as i64);
    Ok(base)
}

/// Buat karakter `nama` dari gambar. Menimpa folder karakter dengan nama sama.
pub fn buat(nama: &str, gambar: &str) -> String {
    let nama: String = nama.trim().to_lowercase().chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
    if nama.is_empty() {
        return "Pakai: /karakter buat <nama> <jalur-gambar.png>".into();
    }
    let base = match siapkan(gambar.trim().trim_matches('"')) {
        Ok(b) => b,
        Err(e) => return e,
    };
    let dir = crate::karakter::root().join(&nama);
    let _ = std::fs::create_dir_all(&dir);
    let save = |name: String, img: &RgbaImage| img.save(dir.join(name)).is_ok();
    let mut n = 0;
    // jalan: goyang + naik-turun
    for (i, (dy, rot)) in [(0.0, -5.0), (-5.0, 0.0), (0.0, 5.0), (-5.0, 0.0)].iter().enumerate() {
        n += save(format!("walk{}.png", i + 1), &xform(&base, 1.0, 1.0, *rot, 0.0, *dy)) as u32;
    }
    // diseret: memanjang, menggantung
    for (i, rot) in [-9.0, 0.0, 9.0].iter().enumerate() {
        n += save(format!("drag{}.png", i + 1), &xform(&base, 0.92, 1.12, *rot, 0.0, -2.0)) as u32;
    }
    // jatuh: berputar kecil
    for (i, rot) in [0.0, 12.0, -12.0].iter().enumerate() {
        n += save(format!("fall{}.png", i + 1), &xform(&base, 0.9, 1.15, *rot, 0.0, -4.0)) as u32;
    }
    // memanjat: miring ke dinding
    for (i, dy) in [0.0, -4.0, 0.0].iter().enumerate() {
        n += save(format!("climb{}.png", i + 1), &xform(&base, 1.0, 1.0, -78.0, -6.0, *dy - 10.0)) as u32;
    }
    // diam 1 "Thinking": napas pelan (3 bingkai)
    for (i, sy) in [1.0, 1.035, 1.0].iter().enumerate() {
        n += save(format!("id1_{}.png", i + 1), &xform(&base, 1.0, *sy, if i == 1 { 2.0 } else { 0.0 }, 0.0, 0.0)) as u32;
    }
    // diam 2 "Sitting": sedikit merunduk
    n += save("id2_1.png".into(), &xform(&base, 1.04, 0.92, 0.0, 0.0, 1.0)) as u32;
    // khusus 1 "spin": berputar horizontal
    for (i, a) in [0.0f32, 45.0, 90.0, 135.0, 180.0, 225.0, 270.0, 315.0].iter().enumerate() {
        n += save(format!("sp1_{}.png", i + 1), &xform(&base, a.to_radians().cos().abs().max(0.12), 1.0, 0.0, 0.0, 0.0)) as u32;
    }
    // khusus 2 "jump": lompat
    for (i, h) in [0.0, -14.0, -26.0, -14.0, 0.0].iter().enumerate() {
        n += save(format!("sp2_{}.png", i + 1), &xform(&base, if *h == 0.0 { 1.06 } else { 0.95 }, if *h == 0.0 { 0.94 } else { 1.06 }, 0.0, 0.0, *h)) as u32;
    }
    // metadata (deskripsi aksi + jumlah putaran)
    let meta_path = crate::karakter::root().join("karakter.json");
    let mut all: serde_json::Value = std::fs::read_to_string(&meta_path).ok().and_then(|t| serde_json::from_str(t.trim_start_matches('\u{feff}')).ok()).unwrap_or_else(|| serde_json::json!({}));
    all[&nama] = serde_json::json!({"displayName": nama, "animation": {
        "idle_actions": {"idle_action_1": {"max_frames": 3, "loop": true, "loop_times": 10, "description": "Thinking"}, "idle_action_2": {"max_frames": 1, "loop": true, "loop_times": 10, "description": "Sitting"}},
        "special_actions": {"special_action_1": {"max_frames": 8, "loop": true, "loop_times": 2, "description": "spin"}, "special_action_2": {"max_frames": 5, "loop": true, "loop_times": 3, "description": "jump"}}}});
    let _ = std::fs::write(&meta_path, serde_json::to_string_pretty(&all).unwrap_or_default());
    let s = bikin_suara();
    format!("🎨 Karakter '{nama}' dibuat: {n} bingkai (jalan, seret, jatuh, panjat, diam, berputar, lompat){}. Pakai: /karakter {nama}", if s > 0 { format!(" + {s} efek suara baru") } else { String::new() })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generates_frames_and_sound() {
        let t = std::env::temp_dir().join(format!("nova_gen_{}.png", std::process::id()));
        let mut img = RgbaImage::new(40, 60);
        for y in 10..60 {
            for x in 5..35 {
                img.put_pixel(x, y, Rgba([200, 60, 60, 255]));
            }
        }
        img.save(&t).unwrap();
        let b = siapkan(&t.to_string_lossy()).unwrap();
        assert_eq!(b.dimensions(), (128, 128));
        let f = xform(&b, 1.0, 1.0, 10.0, 0.0, 0.0);
        assert!(f.pixels().any(|p| p.0[3] > 0));
        let mut s = vec![];
        tone(&mut s, 440.0, 0.05, false, 0.3);
        assert!(s.len() > 1000 && s.iter().any(|v| *v != 0));
        let _ = std::fs::remove_file(t);
    }
}
