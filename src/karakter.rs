//! Karakter gaya Shimeji (diadaptasi dari AkshitIreddy/AI-Desktop-Pet, MIT; mesin Shimeji-ee/Yuki Yamada — lihat catatan di README).
//! Satu karakter = satu folder `karakter/<nama>/` berisi bingkai PNG (umumnya 128×128): `walk1..N`, `drag1..N`, `fall1..N`,
//! `climb1..N`, aksi diam `idK_M` (aksi K bingkai M) dan aksi khusus `spK_M`. Jumlah bingkai dibaca dari nama berkas, deskripsi
//! aksi (mis. "Thinking", "Sitting") dari `karakter.json` bila ada. Karakter aktif disimpan di `karakter/aktif.txt`.
use std::path::{Path, PathBuf};

pub fn root() -> PathBuf {
    let d = crate::config::data_dir().join("karakter");
    let _ = std::fs::create_dir_all(&d);
    d
}

#[derive(Clone, Default, Debug)]
pub struct Aksi {
    pub frames: Vec<PathBuf>,
    pub loops: u32,
    pub desc: String,
}

#[derive(Clone, Default, Debug)]
pub struct Set {
    pub nama: String,
    pub walk: Vec<PathBuf>,
    pub drag: Vec<PathBuf>,
    pub fall: Vec<PathBuf>,
    pub climb: Vec<PathBuf>,
    pub idle: Vec<Aksi>,
    pub special: Vec<Aksi>,
}

/// Daftar karakter yang tersedia (folder yang punya bingkai walk1.png).
pub fn daftar() -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(root()).map(|rd| rd.flatten().filter(|e| e.path().join("walk1.png").exists()).map(|e| e.file_name().to_string_lossy().to_string()).collect()).unwrap_or_default();
    v.sort();
    v
}

pub fn aktif() -> String {
    let n = std::fs::read_to_string(root().join("aktif.txt")).unwrap_or_default().trim().to_lowercase();
    if !n.is_empty() && root().join(&n).join("walk1.png").exists() { n } else { String::new() }
}

pub fn set_aktif(n: &str) -> Result<String, String> {
    let n = n.trim().to_lowercase();
    if ["", "nova", "robot", "default", "bawaan"].contains(&n.as_str()) {
        let _ = std::fs::remove_file(root().join("aktif.txt"));
        return Ok("Kembali ke robot NOVA bawaan 🤖".into());
    }
    if !root().join(&n).join("walk1.png").exists() {
        return Err(format!("Karakter '{n}' tidak ada. Tersedia: {}", daftar().join(", ")));
    }
    std::fs::write(root().join("aktif.txt"), &n).map_err(|e| e.to_string())?;
    Ok(format!("Karakter sekarang: {n} ✨"))
}

fn frames(dir: &Path, prefix: &str) -> Vec<PathBuf> {
    let mut v = vec![];
    for i in 1..=40 {
        let p = dir.join(format!("{prefix}{i}.png"));
        if p.exists() { v.push(p) } else { break }
    }
    v
}

fn aksi(dir: &Path, prefix: &str, desc: &serde_json::Value, key: &str) -> Vec<Aksi> {
    let mut out = vec![];
    for k in 1..=12 {
        let fr: Vec<PathBuf> = (1..=40).map(|m| dir.join(format!("{prefix}{k}_{m}.png"))).take_while(|p| p.exists()).collect();
        if fr.is_empty() {
            break;
        }
        let meta = &desc[format!("{key}_{k}")];
        out.push(Aksi { frames: fr, loops: meta["loop_times"].as_u64().unwrap_or(6).clamp(1, 12) as u32, desc: meta["description"].as_str().unwrap_or("").to_string() });
    }
    out
}

/// Muat definisi karakter dari disk (tanpa tekstur).
pub fn muat(nama: &str) -> Option<Set> {
    let dir = root().join(nama);
    let walk = frames(&dir, "walk");
    if walk.is_empty() {
        return None;
    }
    let meta: serde_json::Value = std::fs::read_to_string(root().join("karakter.json")).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(t.trim_start_matches('\u{feff}')).ok()).map(|v| v[nama]["animation"].clone()).unwrap_or_default();
    Some(Set {
        nama: nama.to_string(),
        drag: frames(&dir, "drag"),
        fall: frames(&dir, "fall"),
        climb: frames(&dir, "climb"),
        idle: aksi(&dir, "id", &meta["idle_actions"], "idle_action"),
        special: aksi(&dir, "sp", &meta["special_actions"], "special_action"),
        walk,
    })
}

/// Salin folder-folder karakter dari `sumber` (mis. public/assets repo AI-Desktop-Pet atau folder buatan sendiri).
pub fn pasang(sumber: &str) -> String {
    let src = PathBuf::from(sumber.trim().trim_matches('"'));
    if !src.is_dir() {
        return format!("Folder '{}' tidak ditemukan.", src.display());
    }
    let mut n = 0;
    let cands: Vec<PathBuf> = if src.join("walk1.png").exists() { vec![src.clone()] } else { std::fs::read_dir(&src).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.join("walk1.png").exists()).collect()).unwrap_or_default() };
    for c in cands {
        let name = c.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        let dst = root().join(&name);
        let _ = std::fs::create_dir_all(&dst);
        if let Ok(rd) = std::fs::read_dir(&c) {
            for e in rd.flatten() {
                if e.path().extension().is_some_and(|x| x == "png") && std::fs::copy(e.path(), dst.join(e.file_name())).is_err() {
                    return format!("Gagal menyalin {}", e.path().display());
                }
            }
            n += 1;
        }
    }
    for j in [src.join("characters.default.json"), src.join("karakter.json"), src.join("..").join("..").join("src").join("shared").join("characters.default.json")] {
        if j.exists() {
            let _ = std::fs::copy(&j, root().join("karakter.json"));
            break;
        }
    }
    if n == 0 { "Tidak ada folder karakter (butuh walk1.png) di sana.".into() } else { format!("{n} karakter dipasang. Pilih: /karakter <nama>. Tersedia: {}", daftar().join(", ")) }
}

pub fn laporan() -> String {
    let d = daftar();
    if d.is_empty() {
        return "Belum ada karakter. Pasang: /karakter pasang <folder berisi folder karakter>".into();
    }
    let a = aktif();
    format!("🎭 Karakter tersedia: {}\nAktif: {}\nGanti: /karakter <nama> · robot bawaan: /karakter nova · pasang baru: /karakter pasang <folder>", d.iter().map(|n| if *n == a { format!("[{n}]") } else { n.clone() }).collect::<Vec<_>>().join(", "), if a.is_empty() { "robot NOVA".into() } else { a })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loads_frames_and_actions() {
        let d = std::env::temp_dir().join(format!("nova_kar_{}", std::process::id())).join("tes");
        std::fs::create_dir_all(&d).unwrap();
        for f in ["walk1", "walk2", "drag1", "fall1", "climb1", "id1_1", "sp1_1", "sp1_2"] {
            std::fs::write(d.join(format!("{f}.png")), b"x").unwrap();
        }
        let s = frames(&d, "walk");
        assert_eq!(s.len(), 2);
        let sp = aksi(&d, "sp", &serde_json::Value::Null, "special_action");
        assert_eq!(sp.len(), 1);
        assert_eq!(sp[0].frames.len(), 2);
        let _ = std::fs::remove_dir_all(d.parent().unwrap());
    }
}
