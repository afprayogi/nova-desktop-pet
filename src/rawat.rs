//! Normalisasi konteks & perluasan kueri (offline): singkatan/slang → baku, tindak-lanjut ("yang rilis tahun ini aja")
//! digabung dengan topik giliran sebelumnya, sinonim untuk pencarian. Buffer 8 giliran terakhir di RAM.
use std::sync::Mutex;

static TURNS: Mutex<Vec<String>> = Mutex::new(Vec::new());

const SLANG: &[(&str, &str)] = &[
    ("yg", "yang"), ("gak", "tidak"), ("ga", "tidak"), ("nggak", "tidak"), ("gk", "tidak"), ("udh", "sudah"), ("udah", "sudah"),
    ("bsk", "besok"), ("skrg", "sekarang"), ("tgl", "tanggal"), ("dgn", "dengan"), ("utk", "untuk"), ("krn", "karena"),
    ("jg", "juga"), ("sm", "sama"), ("dr", "dari"), ("hrs", "harus"), ("blm", "belum"), ("bgt", "banget"), ("tp", "tapi"),
    ("lg", "lagi"), ("aja", "saja"), ("gimana", "bagaimana"), ("kpn", "kapan"), ("brp", "berapa"), ("org", "orang"),
];

const SINONIM: &[(&[&str], &str)] = &[
    (&["film", "movie", "bioskop"], "film movie"),
    (&["murah", "hemat", "diskon"], "murah hemat diskon"),
    (&["berita", "kabar", "terbaru"], "berita terbaru"),
    (&["laptop", "notebook"], "laptop notebook"),
    (&["hp", "ponsel", "smartphone"], "hp ponsel smartphone"),
    (&["error", "galat", "bug"], "error galat bug"),
    (&["resep", "masakan"], "resep masakan"),
    (&["anime", "donghua"], "anime"),
];

/// Kata sambung yang menandakan kalimat melanjutkan topik sebelumnya.
const LANJUT: &[&str] = &["yang", "terus", "lalu", "kalau", "kalo", "bagaimana", "dan", "tapi", "lebih", "yg", "juga", "satu", "coba"];

/// Singkatan/slang → baku, per kata utuh. Tanda baca dan huruf besar kata lain dibiarkan.
pub fn normalisasi(s: &str) -> String {
    s.split_whitespace()
        .map(|w| {
            let core = w.trim_matches(|c: char| !c.is_alphanumeric());
            match SLANG.iter().find(|(k, _)| core.eq_ignore_ascii_case(k)) {
                Some((_, v)) => w.replace(core, v),
                None => w.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Tambahkan sinonim bila kata kunci muncul (untuk pencarian; tidak mengubah isi perintah).
pub fn perluas(q: &str) -> String {
    let l = q.to_lowercase();
    let words: Vec<&str> = l.split(|c: char| !c.is_alphanumeric()).collect();
    let mut out = q.trim().to_string();
    for (keys, ext) in SINONIM {
        if keys.iter().any(|k| words.contains(k)) {
            for e in ext.split(' ') {
                if !words.contains(&e) && !out.to_lowercase().contains(e) {
                    out.push(' ');
                    out.push_str(e);
                }
            }
        }
    }
    out
}

fn tindak_lanjut(l: &str) -> bool {
    let w: Vec<&str> = l.split_whitespace().collect();
    !w.is_empty() && w.len() <= 7 && LANJUT.contains(&w[0]) && !l.starts_with('/')
}

/// Normalisasi lalu, bila ini tindak-lanjut, sambungkan dengan giliran sebelumnya. Giliran dicatat.
pub fn resolve(t: &str) -> String {
    let n = normalisasi(t);
    let l = n.to_lowercase();
    let mut v = TURNS.lock().unwrap();
    let out = match v.last() {
        Some(prev) if tindak_lanjut(&l) => format!("{prev} {n}"),
        _ => n,
    };
    v.push(out.clone());
    if v.len() > 8 {
        v.remove(0);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalizes_and_expands() {
        assert_eq!(normalisasi("yg gak ada, Bsk ya"), "yang tidak ada, besok ya");
        assert!(perluas("film murah").contains("movie"));
        assert!(!perluas("hitung 2+2").contains("movie"));
    }
    #[test]
    fn followup_joins_previous() {
        let _ = resolve("cari anime aksi");
        assert_eq!(resolve("yang rilis tahun ini aja"), "cari anime aksi yang rilis tahun ini saja");
        assert_eq!(resolve("buka notepad"), "buka notepad");
    }
}
