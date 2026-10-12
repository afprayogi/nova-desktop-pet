//! Normalisasi konteks & perluasan kueri (offline): singkatan/slang → baku, tindak-lanjut ("yang rilis tahun ini aja")
//! digabung dengan topik giliran sebelumnya, sinonim untuk pencarian. Buffer 8 giliran terakhir di RAM.
use std::sync::Mutex;

static TURNS: Mutex<Vec<String>> = Mutex::new(Vec::new());


/// Singkatan/slang → baku, per kata utuh. Tanda baca dan huruf besar kata lain dibiarkan.
pub fn normalisasi(s: &str) -> String {
    let lex = crate::lex::bahasa();
    s.split_whitespace()
        .map(|w| {
            let core = w.trim_matches(|c: char| !c.is_alphanumeric());
            match lex.slang.get(&core.to_lowercase()) {
                Some(v) => w.replace(core, v),
                None => match crate::kata::baku(core) {
                    Some(b) => w.replace(core, &b),
                    None => w.to_string(),
                },
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
    for (keys, ext) in &crate::lex::bahasa().sinonim {
        if keys.iter().any(|k| words.contains(&k.as_str())) {
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

fn punya_isi(l: &str) -> bool {
    l.split(|c: char| !c.is_alphanumeric()).any(|w| w.len() > 2 && !crate::lex::ada("buang_koreksi", w) && crate::kata::aksi(w).is_empty())
}

/// Sambung ke giliran sebelumnya hanya bila kalimat ini tidak membawa topik/objek sendiri: koreksi tanpa isi ("carikan di youtube maksudnya"),
/// atau diawali kata sambung ("yang rilis tahun ini aja") TANPA kata kerja sendiri. "coba cari video X" berdiri sendiri.
fn tindak_lanjut(l: &str) -> bool {
    let w: Vec<&str> = l.split_whitespace().collect();
    if w.is_empty() || w.len() > 7 || l.starts_with('/') {
        return false;
    }
    let koreksi = l.contains("maksud") || ["carikan di", "cari di", "di youtube", "di google", "di github", "di maps"].iter().any(|k| l.starts_with(k));
    if koreksi {
        return !punya_isi(l);
    }
    crate::lex::ada("lanjut", w[0]) && w[0] != "coba" && crate::kata::aksi(l).is_empty()
}

static ASLI: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

/// Kalimat asli Bos di balik kalimat gabungan `g` (atau `g` sendiri bila tidak digabung).
pub fn asli(g: &str) -> String {
    ASLI.lock().unwrap().iter().find(|(m, _)| m == g).map(|x| x.1.clone()).unwrap_or_else(|| g.to_string())
}

/// Normalisasi lalu, bila ini tindak-lanjut, sambungkan dengan giliran sebelumnya. Giliran dicatat.
pub fn resolve(t: &str) -> String {
    let n = normalisasi(t);
    let l = n.to_lowercase();
    let mut v = TURNS.lock().unwrap();
    let out = match v.last() {
        Some(prev) if tindak_lanjut(&l) => {
            let g = format!("{prev} {n}");
            let mut m = ASLI.lock().unwrap();
            m.push((g.clone(), n.clone()));
            if m.len() > 30 {
                m.remove(0);
            }
            g
        }
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
        // kalimat yang membawa topik sendiri tidak ditempeli giliran sebelumnya
        let _ = resolve("siapa spongebob");
        assert_eq!(resolve("coba cari video spongebob"), "coba cari video spongebob");
        assert_eq!(resolve("video youtube spongebab maksudnya"), "video youtube spongebab maksudnya");
        let _ = resolve("halo gemini coba buka video anime fullmetal");
        assert!(resolve("carikan di video youtube maksudnya").starts_with("halo gemini coba buka video anime fullmetal"));
    }
}
