//! Putusan: NOVA memverifikasi tebakan lokalnya dengan model (Gemini, atau Llama bila Gemini tidak ada), lalu MENYIMPAN hasilnya di
//! `otak/putusan.json`. Sepakat → terverifikasi; beda → Bos ditanya lewat tombol; dua kali sepakat atau satu konfirmasi Bos →
//! dipromosikan menjadi aturan di `ajar.json`. Kalimat yang sudah terverifikasi dijawab langsung tanpa memanggil model lagi.
//! `kasus.json` (ditulis Bos: [{"kalimat","alat","arg"}]) selalu dipercaya. Entri "ditolak" tidak pernah dipakai ulang.
use crate::alat::Call;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::Instant;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Entri {
    pub kalimat: String,
    pub alat: String,
    pub arg: String,
    pub sumber: String,
    /// model (belum terverifikasi) | terverifikasi | dikonfirmasi | ditolak
    pub status: String,
    pub hitung: u32,
}

pub enum Nilai {
    Sepakat,
    Beda(Call, Call),
    Baru,
}

const KOSONG: &str = "[]";
static TUNGGU: Mutex<Option<(String, Call, Call, [String; 2])>> = Mutex::new(None);
static LAST: Mutex<Option<(String, Call, Instant)>> = Mutex::new(None);
static PAKSA: Mutex<Option<String>> = Mutex::new(None);

pub fn norm(s: &str) -> String {
    crate::rawat::normalisasi(s).to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn daftar() -> Vec<Entri> {
    serde_json::from_value((*crate::data::json("putusan.json", KOSONG)).clone()).unwrap_or_default()
}

fn tulis(f: impl FnOnce(&mut Vec<Entri>)) {
    let _ = crate::data::ubah("putusan.json", KOSONG, |v| {
        let mut d: Vec<Entri> = serde_json::from_value(v.clone()).unwrap_or_default();
        f(&mut d);
        let n = d.len().saturating_sub(400);
        d.drain(..n);
        *v = serde_json::to_value(d).unwrap_or_default();
    });
}

/// Keputusan tersimpan untuk kalimat ini: kasus Bos dulu, lalu putusan terverifikasi/dikonfirmasi.
pub fn cari(kalimat: &str) -> Option<Call> {
    let n = norm(kalimat);
    if let Some(a) = crate::data::json("kasus.json", KOSONG).as_array() {
        if let Some(x) = a.iter().find(|x| x["kalimat"].as_str().is_some_and(|k| norm(k) == n)) {
            return Some(Call { tool: x["alat"].as_str()?.into(), arg: x["arg"].as_str().unwrap_or("").into() });
        }
    }
    daftar().into_iter().find(|e| e.kalimat == n && (e.status == "terverifikasi" || e.status == "dikonfirmasi")).map(|e| Call { tool: e.alat, arg: e.arg })
}

/// Catat/perbarui satu keputusan. Mengembalikan true bila baru saja dipromosikan menjadi aturan.
fn upsert(kalimat: &str, c: &Call, sumber: &str, status: &str) -> bool {
    let n = norm(kalimat);
    let (mut promosi, mut arg) = (false, c.arg.clone());
    tulis(|d| {
        match d.iter_mut().find(|e| e.kalimat == n) {
            Some(e) if e.alat == c.tool => {
                e.hitung += 1;
                e.arg = c.arg.clone();
                e.sumber = sumber.into();
                if status == "dikonfirmasi" || (status == "terverifikasi" && e.status != "dikonfirmasi") || (e.hitung >= 2 && e.status == "model") {
                    e.status = if status == "dikonfirmasi" { "dikonfirmasi".into() } else if e.status == "dikonfirmasi" { e.status.clone() } else { "terverifikasi".into() };
                }
                promosi = (e.status == "terverifikasi" || e.status == "dikonfirmasi") && (e.hitung >= 2 || e.status == "dikonfirmasi");
                arg = e.arg.clone();
            }
            _ => {
                d.retain(|e| e.kalimat != n);
                d.push(Entri { kalimat: n.clone(), alat: c.tool.clone(), arg: c.arg.clone(), sumber: sumber.into(), status: status.into(), hitung: 1 });
                promosi = status == "dikonfirmasi";
            }
        }
    });
    promosi && crate::ajar::tambah(&n, &c.tool, &arg).is_ok()
}

/// Bandingkan tebakan lokal `a` dengan rencana model `b` (satu panggilan) dan simpan hasilnya.
pub fn nilai(goal: &str, a: Option<&Call>, b: &[Call], sumber: &str) -> Nilai {
    let [b] = b else { return Nilai::Baru };
    if b.tool == "tanya" {
        return Nilai::Baru;
    }
    match a.filter(|a| a.tool != "tanya") {
        Some(a) if a.tool == b.tool => {
            upsert(goal, b, sumber, "terverifikasi");
            Nilai::Sepakat
        }
        Some(a) => Nilai::Beda(a.clone(), b.clone()),
        None => {
            // Llama kecil: tidak dianggap terverifikasi sampai muncul dua kali atau Bos mengonfirmasi.
            upsert(goal, b, sumber, "model");
            Nilai::Baru
        }
    }
}

fn label(c: &Call) -> String {
    let a: String = c.arg.chars().take(28).collect();
    if a.is_empty() { c.tool.clone() } else { format!("{} {a}", c.tool) }
}

/// Simpan pertanyaan "maksudmu yang mana?" dan kembalikan teks tombolnya.
pub fn tunggu(goal: &str, a: &Call, b: &Call) -> Vec<String> {
    let o = [format!("Lokal: {}", label(a)), format!("Model: {}", label(b))];
    *TUNGGU.lock().unwrap() = Some((goal.to_string(), a.clone(), b.clone(), o.clone()));
    o.to_vec()
}

/// Jawaban Bos atas tombol di atas. Mengembalikan kalimat asal (untuk dijalankan ulang) dan pesan.
pub fn jawab(t: &str) -> Option<(String, String)> {
    let mut g = TUNGGU.lock().unwrap();
    let (goal, a, b, o) = g.as_ref()?.clone();
    let (benar, salah) = if t == o[0] { (a, b) } else if t == o[1] { (b, a) } else { return None };
    *g = None;
    drop(g);
    let promosi = upsert(&goal, &benar, "bos", "dikonfirmasi");
    tolak_alat(&goal, &salah.tool);
    Some((goal.clone(), format!("📚 Dicatat: \"{}\" = {}.{}", norm(&goal), label(&benar), if promosi { " Sudah jadi aturan." } else { "" })))
}

fn tolak_alat(goal: &str, alat: &str) {
    if let Some(l) = crate::alat::label_of(alat) {
        crate::ml::penalize(goal, l);
    }
}

pub fn set_last(goal: &str, c: &Call) {
    *LAST.lock().unwrap() = Some((goal.to_string(), c.clone(), Instant::now()));
}

/// Koreksi dalam 60 detik ("bukan", "maksudku…", "salah"): tandai keputusan terakhir ditolak dan minta jalankan ulang lewat model.
pub fn koreksi(t: &str) -> Option<String> {
    let w = t.trim().to_lowercase();
    if !["bukan", "maksud", "salah", "bukan itu"].iter().any(|k| w.starts_with(k)) {
        return None;
    }
    let (goal, c, at) = LAST.lock().unwrap().take()?;
    if at.elapsed().as_secs() > 60 {
        return None;
    }
    let n = norm(&goal);
    tulis(|d| {
        if let Some(e) = d.iter_mut().find(|e| e.kalimat == n) {
            e.status = "ditolak".into();
        } else {
            d.push(Entri { kalimat: n.clone(), alat: c.tool.clone(), arg: c.arg.clone(), sumber: "bos".into(), status: "ditolak".into(), hitung: 1 });
        }
    });
    tolak_alat(&goal, &c.tool);
    *PAKSA.lock().unwrap() = Some(goal.clone());
    Some(goal)
}

/// Sekali pakai: kalimat ini harus ditanyakan ke model (abaikan tebakan lokal).
pub fn ambil_paksa(goal: &str) -> bool {
    let mut p = PAKSA.lock().unwrap();
    if p.as_deref() == Some(goal) {
        *p = None;
        true
    } else {
        false
    }
}

/// Contoh terverifikasi paling mirip kalimat (untuk few-shot Llama lokal): [(kalimat, alat, arg)].
pub fn contoh(goal: &str, k: usize) -> Vec<(String, String, String)> {
    let kata: Vec<String> = norm(goal).split(' ').filter(|w| w.len() > 2).map(String::from).collect();
    let mut v: Vec<(usize, Entri)> = daftar()
        .into_iter()
        .filter(|e| e.status == "terverifikasi" || e.status == "dikonfirmasi")
        .map(|e| (kata.iter().filter(|w| e.kalimat.contains(w.as_str())).count(), e))
        .filter(|(s, _)| *s > 0)
        .collect();
    v.sort_by(|a, b| b.0.cmp(&a.0));
    v.into_iter().take(k).map(|(_, e)| (e.kalimat, e.alat, e.arg)).collect()
}

/// Dasbor: setujui (dikonfirmasi + aturan), tolak, atau hapus entri ke-`i`.
pub fn aksi(i: usize, apa: &str) -> bool {
    let d = daftar();
    let Some(e) = d.get(i).cloned() else { return false };
    match apa {
        "setuju" => {
            upsert(&e.kalimat, &Call { tool: e.alat, arg: e.arg }, "bos", "dikonfirmasi");
        }
        "tolak" => tulis(|d| {
            if let Some(x) = d.get_mut(i) {
                x.status = "ditolak".into();
            }
        }),
        _ => tulis(|d| {
            if i < d.len() {
                d.remove(i);
            }
        }),
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    fn c(t: &str, a: &str) -> Call {
        Call { tool: t.into(), arg: a.into() }
    }
    #[test]
    fn agree_verifies_disagree_asks_and_confirm_promotes() {
        let g = format!("uji putusan {}", std::process::id());
        assert!(cari(&g).is_none());
        assert!(matches!(nilai(&g, Some(&c("baterai", "")), &[c("baterai", "")], "gemini"), Nilai::Sepakat));
        assert_eq!(cari(&g).unwrap().tool, "baterai");
        let g2 = format!("uji beda {}", std::process::id());
        let Nilai::Beda(a, b) = nilai(&g2, Some(&c("cari", "x")), &[c("gambar", "x")], "gemini") else { panic!() };
        let o = tunggu(&g2, &a, &b);
        let (goal, pesan) = jawab(&o[1]).unwrap();
        assert_eq!(goal, g2);
        assert!(pesan.contains("gambar"));
        assert_eq!(cari(&g2).unwrap().tool, "gambar");
        assert!(crate::ajar::cocok(&g2).is_some()); // dikonfirmasi → jadi aturan
        assert!(jawab(&o[0]).is_none()); // tidak berlaku dua kali
    }
    #[test]
    fn model_only_needs_two_hits_and_rejection_blocks() {
        let g = format!("uji model {}", std::process::id());
        nilai(&g, None, &[c("waktu", "")], "llama");
        assert!(cari(&g).is_none());
        nilai(&g, None, &[c("waktu", "")], "llama");
        assert_eq!(cari(&g).unwrap().tool, "waktu");
        set_last(&g, &c("waktu", ""));
        assert_eq!(koreksi("bukan itu"), Some(g.clone()));
        assert!(cari(&g).is_none());
        assert!(ambil_paksa(&g) && !ambil_paksa(&g));
    }
}
