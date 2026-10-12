//! Aturan yang DIAJARKAN langsung oleh Bos lewat dasbor (tab Belajar): pola kalimat → alat + argumen.
//! Pola boleh memuat `*` (bagian variabel); `{*}` di argumen diganti isi `*`. Aturan ajar diperiksa lebih dulu daripada
//! aturan bawaan, tetap melewati penilaian risiko `alat::assess`, dan ikut menjadi contoh latihan ML.
use crate::alat::Call;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Aturan {
    pub pola: String,
    pub alat: String,
    pub arg: String,
}

fn file() -> std::path::PathBuf {
    crate::config::data_dir().join("ajar.json")
}

pub fn daftar() -> Vec<Aturan> {
    std::fs::read_to_string(file()).ok().and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default()
}

fn simpan(v: &[Aturan]) {
    let _ = std::fs::write(file(), serde_json::to_string_pretty(v).unwrap_or_default());
}

fn norm(s: &str) -> String {
    crate::rawat::normalisasi(s).to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Cocokkan `kalimat` (sudah dinormalkan) dengan `pola`; hasilnya isi `*` (kosong bila pola tanpa `*`).
fn cocok_pola(pola: &str, kalimat: &str) -> Option<String> {
    match pola.split_once('*') {
        None => (pola == kalimat).then(String::new),
        Some((awal, akhir)) => {
            let (awal, akhir) = (awal.trim_end(), akhir.trim_start());
            let isi = kalimat.strip_prefix(awal)?.trim_start();
            let isi = if akhir.is_empty() { isi } else { isi.strip_suffix(akhir)?.trim_end() };
            (!isi.is_empty()).then(|| isi.to_string())
        }
    }
}

/// Aturan ajar yang cocok untuk kalimat ini (pola yang lebih spesifik/panjang didahulukan).
pub fn cocok(kalimat: &str) -> Option<Call> {
    let k = norm(kalimat);
    let mut v = daftar();
    v.sort_by_key(|a| std::cmp::Reverse(a.pola.len()));
    v.iter().find_map(|a| cocok_pola(&a.pola, &k).map(|isi| Call { tool: a.alat.clone(), arg: a.arg.replace("{*}", &isi) }))
}

/// Tambah/ganti aturan. Mengembalikan pesan hasil (atau galat).
pub fn tambah(pola: &str, alat: &str, arg: &str) -> Result<String, String> {
    let pola = norm(pola);
    if pola.is_empty() || pola.len() > 200 {
        return Err("pola kosong atau terlalu panjang".into());
    }
    if !crate::gemini::tool_names().iter().any(|t| t == alat) {
        return Err(format!("alat '{alat}' tidak dikenal"));
    }
    if arg.len() > 400 {
        return Err("argumen terlalu panjang".into());
    }
    let mut v = daftar();
    v.retain(|a| a.pola != pola);
    v.push(Aturan { pola: pola.clone(), alat: alat.into(), arg: arg.into() });
    simpan(&v);
    // Contoh latihan ML (tanpa tanda *): kalimat nyata menjadi contoh niat bagi alat itu.
    if !pola.contains('*') {
        if let Some(l) = crate::alat::label_of(alat) {
            crate::ml::learn(l, &pola);
        }
    }
    Ok(format!("Tersimpan: \"{pola}\" → {alat} {arg}"))
}

pub fn hapus(i: usize) -> bool {
    let mut v = daftar();
    if i < v.len() {
        v.remove(i);
        simpan(&v);
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pattern_matching() {
        assert_eq!(cocok_pola("buka music *", "buka music anime").as_deref(), Some("anime"));
        assert_eq!(cocok_pola("putar * di youtube", "putar lagu x di youtube").as_deref(), Some("lagu x"));
        assert_eq!(cocok_pola("halo nova", "halo nova").as_deref(), Some(""));
        assert_eq!(cocok_pola("buka music *", "buka music"), None);
        assert_eq!(cocok_pola("halo", "halo nova"), None);
    }
}
