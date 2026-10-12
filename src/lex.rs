//! Data bahasa (`bahasa.json`): kata pengisi, stopword, kata sambung, slang, sinonim. Bawaan tertanam dari `data/bahasa.json`;
//! `otak/bahasa.json` milik Bos MENAMBAH (daftar digabung, kamus ditimpa per kata) dan terbaca ulang saat berkas berubah.
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

pub const BAWAAN: &str = include_str!("../data/bahasa.json");

pub struct Bahasa {
    sets: HashMap<String, HashSet<String>>,
    pub slang: HashMap<String, String>,
    pub sinonim: Vec<(Vec<String>, String)>,
    /// urutan asli (mis. `pengisi` dicocokkan dari depan)
    pub urut: HashMap<String, Vec<String>>,
}

static C: Mutex<Option<(Arc<serde_json::Value>, Arc<Bahasa>)>> = Mutex::new(None);

fn teks(v: &serde_json::Value) -> Vec<String> {
    v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_lowercase())).collect()).unwrap_or_default()
}

pub fn bahasa() -> Arc<Bahasa> {
    let v = crate::data::lapis("bahasa.json", BAWAAN);
    let mut c = C.lock().unwrap();
    if let Some((k, b)) = c.as_ref() {
        if Arc::ptr_eq(k, &v) {
            return b.clone();
        }
    }
    let (mut sets, mut urut, mut slang, mut sinonim) = (HashMap::new(), HashMap::new(), HashMap::new(), vec![]);
    if let Some(o) = v.as_object() {
        for (k, x) in o {
            match k.as_str() {
                "slang" => slang = x.as_object().map(|m| m.iter().filter_map(|(a, b)| Some((a.to_lowercase(), b.as_str()?.to_string()))).collect()).unwrap_or_default(),
                "sinonim" => sinonim = x.as_array().map(|a| a.iter().map(|e| (teks(&e["kata"]), e["tambah"].as_str().unwrap_or("").to_string())).collect()).unwrap_or_default(),
                _ => {
                    let l = teks(x);
                    sets.insert(k.clone(), l.iter().cloned().collect());
                    urut.insert(k.clone(), l);
                }
            }
        }
    }
    let b = Arc::new(Bahasa { sets, slang, sinonim, urut });
    *c = Some((v, b.clone()));
    b
}

impl Bahasa {
    pub fn punya(&self, daftar: &str, kata: &str) -> bool {
        self.sets.get(daftar).is_some_and(|s| s.contains(&kata.to_lowercase()))
    }
}

/// Kata `k` ada di daftar `daftar` (huruf besar/kecil diabaikan).
pub fn ada(daftar: &str, k: &str) -> bool {
    bahasa().punya(daftar, k)
}

#[cfg(test)]
mod tests {
    #[test]
    fn defaults_load_and_lookup() {
        assert!(super::ada("pengisi", "tolong"));
        assert!(super::ada("stop_cari", "yang"));
        assert!(!super::ada("stop_cari", "transformator"));
        assert_eq!(super::bahasa().slang.get("yg").map(String::as_str), Some("yang"));
        assert!(!super::bahasa().sinonim.is_empty());
    }
}
