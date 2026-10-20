//! Paket keahlian bertanda tangan (Ed25519) untuk dibagikan antar-pengguna NOVA. Tanda tangan membuktikan paket tidak diubah dan
//! berasal dari pemilik kunci yang sama (sidik jari ditampilkan); BUKAN jaminan aman: isi tetap divalidasi dan butuh persetujuan.
use crate::config::data_dir;
use ring::signature::{Ed25519KeyPair, KeyPair, UnparsedPublicKey, ED25519};
use serde_json::{json, Value};

const FORMAT: &str = "nova-keahlian-1";

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn dari_hex(s: &str) -> Option<Vec<u8>> {
    (s.len() % 2 == 0).then(|| (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect::<Option<Vec<u8>>>()).flatten()
}

/// Kunci penandatangan lokal; dibuat sekali dan disimpan di berbagi.key (tidak pernah ikut diekspor).
fn kunci() -> Result<Ed25519KeyPair, String> {
    let p = data_dir().join("berbagi.key");
    let pkcs8 = match std::fs::read(&p) {
        Ok(b) => b,
        Err(_) => {
            let b = Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).map_err(|_| "gagal membuat kunci")?.as_ref().to_vec();
            std::fs::write(&p, &b).map_err(|e| e.to_string())?;
            b
        }
    };
    Ed25519KeyPair::from_pkcs8(&pkcs8).map_err(|_| "kunci rusak".to_string())
}

pub fn sidik_jari(pub_hex: &str) -> String {
    pub_hex.chars().take(16).collect::<Vec<_>>().chunks(4).map(|c| c.iter().collect::<String>()).collect::<Vec<_>>().join("-")
}

/// Bungkus daftar keahlian (sudah berupa JSON) menjadi paket bertanda tangan.
pub fn bungkus(items: &[Value]) -> Result<String, String> {
    let k = kunci()?;
    let isi = serde_json::to_vec(items).map_err(|e| e.to_string())?;
    Ok(serde_json::to_string_pretty(&json!({"format": FORMAT, "pembuat": hex(k.public_key().as_ref()), "keahlian": items, "tanda_tangan": hex(k.sign(&isi).as_ref())})).unwrap_or_default())
}

/// Buka paket: (daftar keahlian, keterangan penandatangan). Paket dengan tanda tangan yang salah ditolak.
pub fn buka(v: &Value) -> Result<(Vec<Value>, String), String> {
    if let Some(arr) = v.as_array() {
        return Ok((arr.clone(), "tanpa tanda tangan (berkas lama atau dibuat tangan)".into()));
    }
    if v["format"] != FORMAT {
        return Err("format paket tidak dikenal".into());
    }
    let items = v["keahlian"].as_array().ok_or("paket tidak berisi keahlian")?;
    let pembuat = v["pembuat"].as_str().unwrap_or("");
    let (pk, sig) = (dari_hex(pembuat).ok_or("kunci pembuat rusak")?, dari_hex(v["tanda_tangan"].as_str().unwrap_or("")).ok_or("tanda tangan rusak")?);
    let isi = serde_json::to_vec(items).map_err(|e| e.to_string())?;
    UnparsedPublicKey::new(&ED25519, &pk).verify(&isi, &sig).map_err(|_| "TANDA TANGAN TIDAK COCOK: paket diubah atau palsu, ditolak".to_string())?;
    let sendiri = kunci().map(|k| hex(k.public_key().as_ref()) == pembuat).unwrap_or(false);
    Ok((items.clone(), format!("bertanda tangan sah oleh {}{}", sidik_jari(pembuat), if sendiri { " (kuncimu sendiri)" } else { "; pastikan sidik jari ini milik orang yang kamu percaya" })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paket_sah_diterima_dan_diubah_ditolak() {
        let items = vec![json!({"nama": "Zip folder", "pemicu": ["zipkan folder *"], "langkah": [{"alat": "kompres", "arg": "{*}"}]})];
        let paket: Value = serde_json::from_str(&bungkus(&items).unwrap()).unwrap();
        let (isi, info) = buka(&paket).unwrap();
        assert_eq!(isi.len(), 1);
        assert!(info.contains("sah") && info.contains("kuncimu sendiri"), "{info}");
        let mut rusak = paket.clone();
        rusak["keahlian"][0]["langkah"][0]["arg"] = json!("rm -rf /");
        assert!(buka(&rusak).unwrap_err().contains("TIDAK COCOK"));
        let mut palsu = paket.clone();
        palsu["tanda_tangan"] = json!("00".repeat(64));
        assert!(buka(&palsu).is_err());
        assert!(buka(&json!([{"nama": "x"}])).unwrap().1.contains("tanpa tanda tangan"));
        assert_eq!(sidik_jari("0123456789abcdef0000"), "0123-4567-89ab-cdef");
    }
}
