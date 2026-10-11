//! Berita sesuai minat Bos: RSS publik Google News (hanya judul + sumber + tautan, tanpa akun). Satu-satunya bagian NOVA
//! yang memerlukan internet; seluruh pemilihan minat dan pengurutan dilakukan lokal. Minat dari `/minat a, b` ditambah topik
//! yang sering Bos tanyakan (profil).
use crate::engine::Tx;

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub title: String,
    pub src: String,
    pub link: String,
    pub img: String,
}

fn unescape(s: &str) -> String {
    let s = s.trim();
    let s = s.strip_prefix("<![CDATA[").and_then(|x| x.strip_suffix("]]>")).unwrap_or(s);
    s.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&apos;", "'")
}

fn tag(s: &str, name: &str) -> Option<String> {
    let a = s.find(&format!("<{name}>"))? + name.len() + 2;
    let b = s[a..].find(&format!("</{name}>"))? + a;
    Some(unescape(&s[a..b]))
}

pub fn parse_rss(xml: &str) -> Vec<Item> {
    xml.split("<item>")
        .skip(1)
        .filter_map(|it| {
            let full = tag(it, "title")?;
            let (title, src) = match full.rsplit_once(" - ") {
                Some((t, s)) => (t.trim().to_string(), s.trim().to_string()),
                None => (full.clone(), String::new()),
            };
            let img = it.split("<enclosure").nth(1).and_then(|e| e.split("url=\"").nth(1)).and_then(|u| u.split('"').next()).filter(|u| u.starts_with("http")).unwrap_or("").to_string();
            Some(Item { title, src, link: tag(it, "link").unwrap_or_default(), img })
        })
        .collect()
}

pub fn interests() -> Vec<String> {
    let mut v: Vec<String> = crate::assistant::db()
        .ok()
        .and_then(|c| {
            let _ = c.execute_batch("CREATE TABLE IF NOT EXISTS minat(topik TEXT PRIMARY KEY); CREATE TABLE IF NOT EXISTS news_seen(link TEXT PRIMARY KEY, ts INT);");
            c.prepare("SELECT topik FROM minat").ok().and_then(|mut st| st.query_map([], |r| r.get::<_, String>(0)).ok().map(|r| r.flatten().collect()))
        })
        .unwrap_or_default();
    if v.is_empty() {
        // Belum diatur: pakai topik yang paling sering ditanyakan.
        for l in crate::ingat::profile() {
            if let Some(t) = l.strip_prefix("Topik yang sering ditanyakan: ") {
                v.extend(t.split(", ").take(2).map(String::from));
            }
        }
    }
    v
}

pub fn set_interests(list: &str) -> String {
    let topics: Vec<String> = list.split(|c| c == ',' || c == ';').map(|t| t.trim().to_lowercase()).filter(|t| !t.is_empty() && t.len() < 60).take(8).collect();
    let Ok(c) = crate::assistant::db() else { return "database tidak tersedia".into() };
    let _ = c.execute_batch("CREATE TABLE IF NOT EXISTS minat(topik TEXT PRIMARY KEY);");
    if topics.is_empty() {
        return format!("Minat saat ini: {}. Atur dengan: /minat listrik, rust, plc", interests().join(", "));
    }
    let _ = c.execute("DELETE FROM minat", []);
    for t in &topics {
        let _ = c.execute("INSERT OR IGNORE INTO minat VALUES(?1)", [t]);
    }
    format!("Minat disimpan: {}. Ketik 'berita' untuk melihat kabar terbaru.", topics.join(", "))
}

/// Berita ANTARA yang cocok dengan topik, lengkap dengan gambar (RSS ANTARA menyertakan enclosure foto).
async fn fetch_antara(http: &reqwest::Client, topic: &str) -> Vec<Item> {
    let words: Vec<String> = topic.to_lowercase().split_whitespace().filter(|w| w.len() > 2).map(String::from).collect();
    let mut out = vec![];
    for url in ["https://www.antaranews.com/rss/terkini.xml", "https://www.antaranews.com/rss/tekno.xml", "https://www.antaranews.com/rss/ekonomi.xml"] {
        if let Ok(r) = http.get(url).timeout(std::time::Duration::from_secs(8)).send().await {
            if let Ok(t) = r.text().await {
                out.extend(parse_rss(&t).into_iter().filter(|i| !i.img.is_empty() && words.iter().any(|w| i.title.to_lowercase().contains(w))).map(|mut i| {
                    i.src = "ANTARA".into();
                    i
                }));
            }
        }
        if out.len() >= 3 {
            break;
        }
    }
    out.truncate(3);
    out
}

pub async fn fetch(http: &reqwest::Client, topic: &str) -> Vec<Item> {
    let url = format!("https://news.google.com/rss/search?q={}&hl=id&gl=ID&ceid=ID:id", urlencoding::encode(&format!("{topic} when:7d")));
    match http.get(url).send().await {
        Ok(r) => r.text().await.map(|t| parse_rss(&t)).unwrap_or_default(),
        Err(_) => vec![],
    }
}

fn seen(link: &str, mark: bool) -> bool {
    let Ok(c) = crate::assistant::db() else { return false };
    let _ = c.execute_batch("CREATE TABLE IF NOT EXISTS news_seen(link TEXT PRIMARY KEY, ts INT);");
    if mark {
        c.execute("INSERT OR IGNORE INTO news_seen VALUES(?1, strftime('%s','now'))", [link]).map(|n| n == 0).unwrap_or(false)
    } else {
        c.query_row("SELECT 1 FROM news_seen WHERE link=?1", [link], |_| Ok(())).is_ok()
    }
}

/// Teks berita untuk satu topik, atau semua minat bila `topic` kosong.
pub async fn headlines(http: &reqwest::Client, topic: &str, only_new: bool) -> (String, Vec<Item>) {
    let topics = if topic.trim().is_empty() { interests() } else { vec![topic.trim().to_string()] };
    if topics.is_empty() {
        return ("Belum ada minat. Atur dengan: /minat listrik, rust, plc (atau tanyakan: berita listrik).".into(), vec![]);
    }
    let (mut out, mut all) = (String::new(), vec![]);
    for t in topics.iter().take(3) {
        let mut items: Vec<Item> = fetch_antara(http, t).await.into_iter().filter(|i| !only_new || !seen(&i.link, true)).collect();
        let more = 3usize.saturating_sub(items.len());
        items.extend(fetch(http, t).await.into_iter().filter(|i| !only_new || !seen(&i.link, true)).take(more));
        if items.is_empty() {
            continue;
        }
        out.push_str(&format!("📰 {t}\n"));
        for (n, i) in items.iter().enumerate() {
            if !i.img.is_empty() {
                out.push_str(&format!("IMG {}\n", i.img));
            }
            out.push_str(&format!("{}. {}{}\n   {}\n", n + 1, i.title, if i.src.is_empty() { String::new() } else { format!(" — {}", i.src) }, i.link));
        }
        all.extend(items);
    }
    if out.is_empty() {
        out = "Tidak ada berita baru (atau tidak ada koneksi internet).".into();
    }
    (out, all)
}

/// Ringkasan harian proaktif (dipanggil asisten; dibatasi bandit).
pub async fn digest(tx: &Tx, http: &reqwest::Client) {
    let (_, items) = headlines(http, "", true).await;
    if items.is_empty() {
        return;
    }
    let lines: Vec<(usize, String)> = items.iter().take(3).map(|i| (3, format!("📰 {}{}", i.title, if i.src.is_empty() { String::new() } else { format!(" — {}", i.src) }))).collect();
    crate::notify::toast("NOVA - berita sesuai minat", &lines[0].1);
    crate::adegan::play_soft(tx, lines, 1, 20);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rss_parsing() {
        let x = "<rss><channel><item><title><![CDATA[PLN tambah daya PLTS - Kompas]]></title><link>https://x.id/a?b=1&amp;c=2</link></item><item><title>Rust 1.90 rilis - Dev.to</title><link>https://y.io</link></item></channel></rss>";
        let v = parse_rss(x);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0], Item { title: "PLN tambah daya PLTS".into(), src: "Kompas".into(), link: "https://x.id/a?b=1&c=2".into(), img: String::new() });
        assert_eq!(v[1].src, "Dev.to");
    }
}
