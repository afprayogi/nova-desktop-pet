//! Riwayat browser (Chrome/Edge) — OPT-IN (`/riwayat on`). Berkas `History` disalin ke folder sementara lalu dibaca hanya-baca.
//! Yang diambil hanya domain, judul, dan jumlah kunjungan 7 hari terakhir; tidak ada isi halaman, kata sandi, atau cookie,
//! dan tidak ada yang disimpan di disk selain bendera aktif/mati.
use std::collections::HashMap;

fn flag() -> std::path::PathBuf {
    crate::config::data_dir().join("riwayat_on")
}

pub fn aktif() -> bool {
    flag().exists()
}

pub fn set(on: bool) -> String {
    if on {
        let _ = std::fs::write(flag(), "");
        "Riwayat browser: AKTIF. Kubaca domain, judul, dan jumlah kunjungan 7 hari terakhir (hanya di laptop ini, tidak disimpan). Ketik /riwayat untuk ringkasan.".into()
    } else {
        let _ = std::fs::remove_file(flag());
        "Riwayat browser: MATI.".into()
    }
}

fn berkas() -> Vec<std::path::PathBuf> {
    let Some(base) = dirs::data_local_dir() else { return vec![] };
    ["Google/Chrome", "Microsoft/Edge"].iter().map(|b| base.join(b).join("User Data/Default/History")).filter(|p| p.exists()).collect()
}

pub struct Baris {
    pub url: String,
    pub judul: String,
    pub kunjungan: u32,
}

/// Baris riwayat `hari` hari terakhir dari semua browser yang ditemukan.
pub fn baca(hari: i64) -> Vec<Baris> {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let batas = (now - hari * 86400 + 11_644_473_600) * 1_000_000; // epoch Chrome: mikrodetik sejak 1601
    let mut out = vec![];
    for (i, src) in berkas().iter().enumerate() {
        let tmp = std::env::temp_dir().join(format!("nova-hist-{i}.db"));
        if std::fs::copy(src, &tmp).is_err() {
            continue;
        }
        if let Ok(c) = rusqlite::Connection::open_with_flags(&tmp, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) {
            if let Ok(mut st) = c.prepare("SELECT url, title, visit_count FROM urls WHERE last_visit_time > ?1 ORDER BY last_visit_time DESC LIMIT 4000") {
                if let Ok(rows) = st.query_map([batas], |r| Ok(Baris { url: r.get(0)?, judul: r.get::<_, String>(1).unwrap_or_default(), kunjungan: r.get::<_, i64>(2).unwrap_or(1).max(1) as u32 })) {
                    out.extend(rows.flatten());
                }
            }
        }
        let _ = std::fs::remove_file(&tmp);
    }
    out
}

fn domain(url: &str) -> String {
    url.split("://").nth(1).unwrap_or(url).split(['/', '?', '#']).next().unwrap_or("").trim_start_matches("www.").to_string()
}

const MUSIK_KATA: &[&str] = &["official", "lyrics", "lirik", "mv", "audio", "ost", "cover", "lagu", "music", "musik", "remix", "live", "acoustic"];

/// Judul lagu dari riwayat (music.youtube.com, atau video YouTube bernuansa musik), urut jumlah kunjungan.
pub fn lagu() -> Vec<(String, u32)> {
    let mut m: HashMap<String, u32> = HashMap::new();
    for b in baca(30) {
        let d = domain(&b.url);
        let ytm = d == "music.youtube.com";
        let yt = d.ends_with("youtube.com") && b.url.contains("/watch");
        if !(ytm || yt) {
            continue;
        }
        let t = b.judul.trim_end_matches(" - YouTube Music").trim_end_matches(" - YouTube").trim().to_lowercase();
        if t.is_empty() || (!ytm && !t.split(|c: char| !c.is_alphanumeric()).any(|w| MUSIK_KATA.contains(&w))) {
            continue;
        }
        *m.entry(t).or_insert(0) += b.kunjungan;
    }
    let mut v: Vec<_> = m.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    v
}

pub fn ringkas() -> String {
    if !aktif() {
        return "Riwayat browser belum diaktifkan. Ketik /riwayat on untuk mengizinkan NOVA membaca domain & judul 7 hari terakhir (hanya lokal).".into();
    }
    let rows = baca(7);
    if rows.is_empty() {
        return "Tidak ada riwayat yang terbaca (Chrome/Edge profil Default tidak ditemukan atau berkas tidak bisa disalin).".into();
    }
    let mut dom: HashMap<String, u32> = HashMap::new();
    for b in &rows {
        *dom.entry(domain(&b.url)).or_insert(0) += b.kunjungan;
    }
    let mut d: Vec<_> = dom.into_iter().filter(|(k, _)| !k.is_empty()).collect();
    d.sort_by(|a, b| b.1.cmp(&a.1));
    let atas: Vec<String> = d.iter().take(8).map(|(k, n)| format!("{k} ({n})")).collect();
    let lg: Vec<String> = lagu().into_iter().take(5).map(|(t, n)| format!("{t} ({n}x)")).collect();
    format!("7 hari terakhir: {} halaman. Situs teratas: {}.{}", rows.len(), atas.join(", "), if lg.is_empty() { String::new() } else { format!(" Lagu teratas: {}.", lg.join("; ")) })
}

#[cfg(test)]
mod tests {
    #[test]
    fn domain_parsing() {
        assert_eq!(super::domain("https://www.youtube.com/watch?v=abc"), "youtube.com");
        assert_eq!(super::domain("https://music.youtube.com/watch?v=x"), "music.youtube.com");
    }
}
