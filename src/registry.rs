//! Satu daftar alat berskema (sumber tunggal): nama, petunjuk argumen, deskripsi, dan tingkat risiko. Bawaan dari `data/alat.json`
//! (tertanam); `otak/alat.json` milik Bos menambah atau menimpa per nama dan terbaca ulang otomatis. Dipakai untuk (1) prompt
//! perencana Gemini/Llama, (2) pencarian alat relevan bagi Llama kecil (embedding), dan (3) ekspor berbentuk MCP (`tools/list`).
//! Alat baru cukup ditambah satu entri; penilaian risiko dan eksekusinya tetap di `alat.rs`.
use serde::Serialize;
use std::sync::{Arc, Mutex};

pub const BAWAAN: &str = include_str!("../data/alat.json");

#[derive(Serialize, Clone, Debug)]
pub struct Skema {
    pub nama: String,
    pub arg: String,
    pub deskripsi: String,
    pub risiko: String,
}

struct Isi {
    skema: Vec<Skema>,
    vek: Vec<Vec<f32>>,
}

static C: Mutex<Option<(Arc<serde_json::Value>, Arc<Isi>)>> = Mutex::new(None);

fn isi() -> Arc<Isi> {
    let v = crate::data::lapis("alat.json", BAWAAN);
    let mut c = C.lock().unwrap();
    if let Some((k, i)) = c.as_ref() {
        if Arc::ptr_eq(k, &v) {
            return i.clone();
        }
    }
    let mut skema: Vec<Skema> = v
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    let n = x["nama"].as_str()?.trim().to_lowercase();
                    if n.is_empty() || !n.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
                        return None;
                    }
                    let r = match crate::alat::assess(&crate::alat::Call { tool: n.clone(), arg: "x".into() }) {
                        crate::alat::Risk::Safe => "aman",
                        crate::alat::Risk::Confirm(_) => "butuh persetujuan",
                        crate::alat::Risk::Block(_) => "bergantung argumen", // probe "x" ditolak validasi argumen, bukan alatnya
                    };
                    Some(Skema { nama: n, arg: x["arg"].as_str().unwrap_or("").into(), deskripsi: x["deskripsi"].as_str().unwrap_or("").into(), risiko: r.into() })
                })
                .collect()
        })
        .unwrap_or_default();
    skema.sort_by(|a, b| a.nama.cmp(&b.nama));
    skema.dedup_by(|a, b| a.nama == b.nama);
    let vek = skema.iter().map(|s| crate::ingat::embed(&format!("{} {} {}", s.nama, s.arg, s.deskripsi))).collect();
    let i = Arc::new(Isi { skema, vek });
    *c = Some((v, i.clone()));
    i
}

pub fn semua() -> Vec<Skema> {
    isi().skema.clone()
}

pub fn ada(nama: &str) -> bool {
    isi().skema.iter().any(|s| s.nama == nama)
}

/// `k` alat paling relevan untuk kalimat `q` (cosine embedding hashing + tumpang-tindih kata).
pub fn relevan(q: &str, k: usize) -> Vec<Skema> {
    let i = isi();
    let qv = crate::ingat::embed(q);
    let kata: Vec<String> = q.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| w.len() > 2).map(String::from).collect();
    let mut s: Vec<(f32, &Skema)> = i
        .skema
        .iter()
        .zip(&i.vek)
        .map(|(sk, v)| {
            let cos: f32 = v.iter().zip(&qv).map(|(a, b)| a * b).sum();
            let teks = format!("{} {}", sk.nama, sk.deskripsi).to_lowercase();
            let lex = kata.iter().filter(|w| teks.contains(w.as_str())).count() as f32 / kata.len().max(1) as f32;
            (cos + 0.6 * lex, sk)
        })
        .collect();
    s.sort_by(|a, b| b.0.total_cmp(&a.0));
    s.into_iter().take(k).map(|(_, sk)| sk.clone()).collect()
}

pub fn baris(s: &Skema) -> String {
    format!("{}{}: {}{}", s.nama, if s.arg.is_empty() { String::new() } else { format!(" {}", s.arg) }, s.deskripsi, if s.risiko == "butuh persetujuan" { " [butuh persetujuan]".to_string() } else { String::new() })
}

/// Semua alat, satu baris per alat (untuk prompt model).
pub fn teks(_: Option<()>) -> String {
    bantuan()
}

pub fn bantuan() -> String {
    isi().skema.iter().map(baris).collect::<Vec<_>>().join("\n")
}

/// Bentuk MCP `tools/list`: tiap alat punya satu parameter string `arg`.
pub fn mcp() -> serde_json::Value {
    serde_json::json!({"tools": semua().iter().map(|s| serde_json::json!({
        "name": s.nama,
        "description": s.deskripsi,
        "inputSchema": {"type": "object", "properties": {"arg": {"type": "string", "description": if s.arg.is_empty() { "tidak dipakai".to_string() } else { s.arg.clone() }}}},
        "annotations": {"risiko": s.risiko, "readOnlyHint": s.risiko == "aman"},
    })).collect::<Vec<_>>()})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registry_covers_tools_and_retrieves() {
        assert!(semua().len() > 50);
        assert!(semua().iter().any(|s| s.nama == "kompres" && s.risiko == "butuh persetujuan"));
        assert!(ada("cariisi") && ada("wifi") && ada("bukafile"));
        let top: Vec<String> = relevan("cek sisa baterai laptop", 6).into_iter().map(|s| s.nama).collect();
        assert!(top.contains(&"baterai".to_string()), "{top:?}");
        assert_eq!(mcp()["tools"][0]["inputSchema"]["type"], "object");
    }
}
