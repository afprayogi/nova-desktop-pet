//! Satu daftar alat berskema (sumber tunggal): nama, petunjuk argumen, deskripsi, dan tingkat risiko, diturunkan otomatis dari
//! `alat::TOOL_HELP` + `gemini::EXTRA_TOOLS`. Dipakai untuk (1) prompt perencana Gemini, (2) pencarian alat relevan bagi Llama
//! kecil (embedding), dan (3) ekspor berbentuk MCP (`tools/list`: name, description, inputSchema, annotations) sebagai fondasi
//! klien/server MCP. Alat baru cukup ditambah satu baris deskripsi; tidak perlu aturan kata kunci.
use serde::Serialize;
use std::sync::OnceLock;

#[derive(Serialize, Clone, Debug)]
pub struct Skema {
    pub nama: String,
    pub arg: String,
    pub deskripsi: String,
    pub risiko: String,
}

fn titik_dua(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    (0..b.len()).find(|&i| b[i] == b':' && (i == 0 || b[i - 1] != b':') && b.get(i + 1) != Some(&b':'))
}

fn nama_sah(w: &str) -> bool {
    !w.is_empty() && w.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Urai satu baris "nama <arg> : deskripsi" (boleh beberapa nama dipisah '/', berbagi deskripsi).
pub fn urai_baris(l: &str) -> Vec<Skema> {
    let Some(i) = titik_dua(l) else { return vec![] };
    let (kiri, desk) = (l[..i].trim(), l[i + 1..].trim());
    if desk.is_empty() {
        return vec![];
    }
    kiri.split(" / ")
        .filter_map(|seg| {
            let seg = seg.trim();
            let (n, arg) = seg.split_once(char::is_whitespace).map(|(a, b)| (a, b.trim())).unwrap_or((seg, ""));
            nama_sah(n).then(|| {
                let r = match crate::alat::assess(&crate::alat::Call { tool: n.into(), arg: "x".into() }) {
                    crate::alat::Risk::Safe => "aman",
                    crate::alat::Risk::Confirm(_) => "butuh persetujuan",
                    crate::alat::Risk::Block(_) => "bergantung argumen", // probe "x" ditolak validasi argumen, bukan alatnya
                };
                Skema { nama: n.into(), arg: arg.into(), deskripsi: desk.into(), risiko: r.into() }
            })
        })
        .collect()
}

pub fn semua() -> &'static Vec<Skema> {
    static R: OnceLock<Vec<Skema>> = OnceLock::new();
    R.get_or_init(|| {
        let mut v: Vec<Skema> = crate::alat::TOOL_HELP.lines().chain(crate::gemini::EXTRA_TOOLS.lines()).flat_map(urai_baris).collect();
        v.sort_by(|a, b| a.nama.cmp(&b.nama));
        v.dedup_by(|a, b| a.nama == b.nama);
        v
    })
}

fn vektor() -> &'static Vec<Vec<f32>> {
    static V: OnceLock<Vec<Vec<f32>>> = OnceLock::new();
    V.get_or_init(|| semua().iter().map(|s| crate::ingat::embed(&format!("{} {} {}", s.nama, s.arg, s.deskripsi))).collect())
}

/// `k` alat paling relevan untuk kalimat `q` (cosine embedding hashing + tumpang-tindih kata).
pub fn relevan(q: &str, k: usize) -> Vec<&'static Skema> {
    let qv = crate::ingat::embed(q);
    let kata: Vec<String> = q.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| w.len() > 2).map(String::from).collect();
    let mut s: Vec<(f32, &Skema)> = semua()
        .iter()
        .zip(vektor())
        .map(|(sk, v)| {
            let cos: f32 = v.iter().zip(&qv).map(|(a, b)| a * b).sum();
            let teks = format!("{} {}", sk.nama, sk.deskripsi).to_lowercase();
            let lex = kata.iter().filter(|w| teks.contains(w.as_str())).count() as f32 / kata.len().max(1) as f32;
            (cos + 0.6 * lex, sk)
        })
        .collect();
    s.sort_by(|a, b| b.0.total_cmp(&a.0));
    s.into_iter().take(k).map(|(_, sk)| sk).collect()
}

pub fn baris(s: &Skema) -> String {
    format!("{}{}: {}{}", s.nama, if s.arg.is_empty() { String::new() } else { format!(" {}", s.arg) }, s.deskripsi, if s.risiko == "butuh persetujuan" { " [butuh persetujuan]".to_string() } else { String::new() })
}

/// Teks daftar alat untuk prompt: semua alat, atau hanya `daftar` (hasil `relevan`).
pub fn teks(daftar: Option<&[&Skema]>) -> String {
    match daftar {
        Some(d) => d.iter().map(|s| baris(s)).collect::<Vec<_>>().join("\n"),
        None => semua().iter().map(baris).collect::<Vec<_>>().join("\n"),
    }
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
    fn parses_lines() {
        let v = urai_baris("wifi / port / bukafile <nama>: lihat Wi-Fi, port aktif, buka berkas");
        assert_eq!(v.iter().map(|s| s.nama.as_str()).collect::<Vec<_>>(), ["wifi", "port", "bukafile"]);
        assert_eq!(v[2].arg, "<nama>");
        let c = urai_baris("cariisi <berkas> :: <pertanyaan>: potongan relevan");
        assert_eq!(c[0].nama, "cariisi");
        assert_eq!(c[0].deskripsi, "potongan relevan");
        assert!(urai_baris("tanpa titik dua").is_empty());
    }
    #[test]
    fn registry_covers_tools_and_retrieves() {
        assert!(semua().len() > 30);
        assert!(semua().iter().any(|s| s.nama == "kompres" && s.risiko == "butuh persetujuan"));
        let top: Vec<&str> = relevan("cek sisa baterai laptop", 6).iter().map(|s| s.nama.as_str()).collect();
        assert!(top.contains(&"baterai"), "{top:?}");
        assert_eq!(mcp()["tools"][0]["inputSchema"]["type"], "object");
    }
}
