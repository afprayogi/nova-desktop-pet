//! Wikipedia Indonesia (dan Wiktionary Indonesia sebagai kamus) → otak. Teks artikel diambil lewat API publik MediaWiki (CC BY-SA),
//! dipotong, dan disimpan ke pengetahuan dengan sumber `wikipedia:<judul>` / `kamus:<kata>`. Daftar topik teknik ada di `topik_teknik.json`.
use serde_json::Value;

pub const TOPIK_BAWAAN: &str = r#"["Motor induksi","Motor listrik","Transformator","Generator listrik","Kontaktor","Relai","Pemutus sirkuit","Sekering","Pengaman lebih arus","Pembumian listrik","Arus listrik","Tegangan listrik","Hambatan listrik","Daya listrik","Faktor daya","Listrik tiga fase","Arus bolak-balik","Arus searah","Hukum Ohm","Hukum Kirchhoff","Kapasitor","Induktor","Dioda","Transistor","Op-amp","Rangkaian terpadu","Mikrokontroler","Arduino","Programmable logic controller","Diagram tangga","Sensor","Aktuator","Pengendali PID","Inverter","Penyearah","Catu daya","Variable-frequency drive","Soft starter","Motor servo","Motor langkah","Panel listrik","Kabel listrik","Instalasi listrik","Persyaratan Umum Instalasi Listrik","Tenaga surya","Baterai","Pembangkit listrik","Jaringan listrik","Gardu listrik","Proteksi sistem tenaga listrik","SCADA","Otomasi industri","Robotika","Pneumatik","Hidrolika","Termokopel","Pengukuran listrik","Osiloskop","Multimeter","Komunikasi serial","Modbus","Ethernet","Protokol internet","Sistem operasi","Basis data","Bahasa pemrograman","Rust (bahasa pemrograman)","Python (bahasa pemrograman)","Git","Kecerdasan buatan","Pembelajaran mesin","Jaringan saraf tiruan"]"#;

/// Bagian teks polos terpanjang dari balasan `action=query&prop=extracts` (judul, isi).
pub fn urai(v: &Value) -> Option<(String, String)> {
    let p = v["query"]["pages"].as_object()?.values().next()?;
    if p.get("missing").is_some() {
        return None;
    }
    let isi = p["extract"].as_str()?.trim();
    (isi.len() > 80).then(|| (p["title"].as_str().unwrap_or("").to_string(), isi.to_string()))
}

async fn ambil(http: &reqwest::Client, host: &str, judul: &str) -> Option<(String, String)> {
    let url = format!("https://{host}/w/api.php?action=query&prop=extracts&explaintext=1&redirects=1&format=json&titles={}", urlencoding::encode(judul));
    let v: Value = http.get(url).header("User-Agent", "nova-asisten (github.com/afprayogi/nova-desktop-pet)").timeout(std::time::Duration::from_secs(25)).send().await.ok()?.json().await.ok()?;
    urai(&v)
}

async fn judul_terbaik(http: &reqwest::Client, lang: &str, q: &str) -> Option<String> {
    let url = format!("https://{lang}.wikipedia.org/w/api.php?action=opensearch&search={}&limit=1&format=json", urlencoding::encode(q));
    let v: Value = http.get(url).header("User-Agent", "nova-asisten").timeout(std::time::Duration::from_secs(15)).send().await.ok()?.json().await.ok()?;
    v[1].get(0)?.as_str().map(String::from)
}

fn simpan(src: String, teks: String) -> usize {
    crate::teknik::ingest_max(&src, &teks.chars().take(60_000).collect::<String>(), 300)
}

/// Cari `q` di Wikipedia (Indonesia dulu; bila tidak ada, Inggris lalu diterjemahkan) dan simpan artikelnya ke otak.
pub async fn cari_simpan(http: &reqwest::Client, q: &str) -> String {
    let q = q.trim();
    if q.len() < 2 {
        return "Topik kosong.".into();
    }
    for lang in ["id", "en"] {
        let Some(judul) = judul_terbaik(http, lang, q).await else { continue };
        let Some((judul, isi)) = ambil(http, &format!("{lang}.wikipedia.org"), &judul).await else { continue };
        let (src, teks) = if lang == "id" {
            (format!("wikipedia:{judul}"), isi)
        } else {
            let awal: String = isi.chars().take(4000).collect();
            match crate::terjemah::terjemahkan(http, &awal, Some("id")).await {
                Ok((t, _, _)) => (format!("wikipedia:{judul} (terjemahan)"), t),
                Err(_) => continue,
            }
        };
        let n = tokio::task::spawn_blocking(move || simpan(src, teks)).await.unwrap_or(0);
        crate::ingat::clear_cache();
        return format!("📖 '{judul}' (Wikipedia {lang}) masuk otak: {n} potongan.");
    }
    format!("Tidak ada artikel Wikipedia untuk '{q}'.")
}

/// Arti kata dari Wiktionary Indonesia → otak.
pub async fn kamus(http: &reqwest::Client, kata: &str) -> String {
    let k = kata.trim().to_lowercase();
    if k.is_empty() || k.split_whitespace().count() > 3 {
        return "Pakai: /arti <kata>".into();
    }
    match ambil(http, "id.wiktionary.org", &k).await {
        Some((judul, isi)) => {
            let n = {
                let (src, t) = (format!("kamus:{judul}"), isi.clone());
                tokio::task::spawn_blocking(move || crate::teknik::ingest_max(&src, &t.chars().take(8000).collect::<String>(), 20)).await.unwrap_or(0)
            };
            crate::ingat::clear_cache();
            format!("📖 {judul} (Wiktionary Indonesia, disimpan ke otak: {n} potongan):\n{}", crate::research::truncate(&isi, 900))
        }
        None => format!("'{k}' tidak ada di Wiktionary Indonesia."),
    }
}

/// Muat topik teknik yang belum ada di otak (maks `n`), satu per satu.
pub async fn isi_topik(http: &reqwest::Client, n: usize) -> String {
    let topik: Vec<String> = crate::data::json("topik_teknik.json", TOPIK_BAWAAN).as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
    let ada: std::collections::HashSet<String> = tokio::task::spawn_blocking(|| {
        crate::teknik::open().ok().and_then(|c| c.prepare("SELECT DISTINCT src FROM kb WHERE src LIKE 'wikipedia:%' OR src LIKE 'tidak-ada:%'").ok().and_then(|mut s| s.query_map([], |r| r.get::<_, String>(0)).ok().map(|r| r.flatten().collect()))).unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    let belum: Vec<&String> = topik.iter().filter(|t| !ada.iter().any(|s| s.eq_ignore_ascii_case(&format!("wikipedia:{t}")) || s.eq_ignore_ascii_case(&format!("tidak-ada:{t}")))).collect();
    if belum.is_empty() {
        return "Semua topik teknik sudah ada di otak. Tambah topik di topik_teknik.json atau minta: cari wikipedia <topik>.".into();
    }
    let (mut ok, mut gagal) = (0, vec![]);
    for t in belum.iter().take(n) {
        let r = cari_simpan(http, t).await;
        if r.starts_with("📖") {
            ok += 1;
        } else {
            gagal.push(t.to_string());
            let tt = format!("tidak-ada:{t}");
            let _ = tokio::task::spawn_blocking(move || crate::teknik::ingest_max(&tt, "(tidak ditemukan di Wikipedia)", 1)).await;
        }
    }
    format!("{ok} artikel Wikipedia masuk otak{}; sisa {} topik.", if gagal.is_empty() { String::new() } else { format!(" (tidak ketemu: {})", gagal.join(", ")) }, belum.len().saturating_sub(n))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_extract_and_missing() {
        let v: Value = serde_json::from_str(&format!(r#"{{"query":{{"pages":{{"123":{{"title":"Transformator","extract":"{}"}}}}}}}}"#, "Transformator adalah alat listrik. ".repeat(5))).unwrap();
        assert_eq!(urai(&v).unwrap().0, "Transformator");
        let m: Value = serde_json::from_str(r#"{"query":{"pages":{"-1":{"title":"X","missing":""}}}}"#).unwrap();
        assert!(urai(&m).is_none());
    }
    #[test]
    fn default_topics_are_valid_json() {
        assert!(serde_json::from_str::<Vec<String>>(TOPIK_BAWAAN).unwrap().len() > 50);
    }
}
