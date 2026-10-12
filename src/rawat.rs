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

/// Kata yang tidak membawa topik baru; kalimat koreksi yang hanya berisi kata-kata ini memang meminta topik sebelumnya.
const BUANG: &[&str] = &["di", "ke", "yang", "itu", "ini", "saja", "aja", "maksudnya", "maksud", "maksudku", "video", "youtube", "yt", "google", "github", "maps", "lagu", "musik", "dong", "kamu", "coba", "salah", "bukan", "jawabannya", "tadi"];

fn punya_isi(l: &str) -> bool {
    l.split(|c: char| !c.is_alphanumeric()).any(|w| w.len() > 2 && !BUANG.contains(&w) && crate::kata::aksi(w).is_empty())
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
    LANJUT.contains(&w[0]) && w[0] != "coba" && crate::kata::aksi(l).is_empty()
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
