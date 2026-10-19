use anyhow::Result;
use serde_json::Value;
use std::time::Duration;

#[derive(Clone)]
pub struct ImageHit {
    pub title: String,
    pub image_url: String,
    pub source_url: String,
    pub license: String,
}

pub struct Hit {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

pub fn client(timeout_s: u64) -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) Nova/0.1")
        .timeout(Duration::from_secs(timeout_s.max(5)))
        .build()
        .expect("http client")
}

pub fn should_auto_research(query: &str) -> bool {
    let q = query.to_lowercase();
    [
        "terbaru",
        "terkini",
        "saat ini",
        "sekarang",
        "hari ini",
        "berita",
        "sumber",
        "referensi",
        "riset",
        "cari di internet",
        "data terbaru",
        "cek fakta",
        "up to date",
        "latest",
        "current",
        "today",
    ]
    .iter()
    .any(|term| q.contains(term))
}

pub fn visual_query(query: &str) -> Option<&str> {
    let lower = query.to_lowercase();
    ["foto", "gambar", "meme", "ilustrasi", "infografik"]
        .iter()
        .find(|term| lower.contains(**term))
        .map(|_| query)
}

pub async fn search_images(http: &reqwest::Client, query: &str, limit: usize) -> Result<Vec<ImageHit>> {
    let response: Value = http
        .get("https://commons.wikimedia.org/w/api.php")
        .query(&[
            ("action", "query"),
            ("generator", "search"),
            ("gsrnamespace", "6"),
            ("gsrlimit", &limit.clamp(1, 3).to_string()),
            ("gsrsearch", &format!("filetype:bitmap {}", query)),
            ("prop", "imageinfo"),
            ("iiprop", "url|extmetadata"),
            ("iiurlwidth", "480"),
            ("format", "json"),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let Some(pages) = response["query"]["pages"].as_object() else {
        return Ok(Vec::new());
    };
    Ok(pages
        .values()
        .filter_map(|page| {
            let image = page["imageinfo"].as_array()?.first()?;
            let image_url = image["thumburl"].as_str()?;
            let source_url = image["descriptionurl"].as_str()?;
            let license = image["extmetadata"]["LicenseShortName"]["value"].as_str()?;
            let title = page["title"].as_str()?.strip_prefix("File:")?;
            (image_url.starts_with("https://upload.wikimedia.org/")
                && source_url.starts_with("https://commons.wikimedia.org/wiki/")
                && reusable_license(license))
            .then(|| ImageHit {
                title: title.to_string(),
                image_url: image_url.to_string(),
                source_url: source_url.to_string(),
                license: strip_tags(license),
            })
        })
        .take(limit.clamp(1, 3))
        .collect())
}

fn reusable_license(license: &str) -> bool {
    let license = license.to_ascii_lowercase();
    license.contains("public domain")
        || license.contains("cc0")
        || (license.contains("cc by") && !license.contains("nc"))
}

/// Gunakan Google Programmable Search jika dikonfigurasi, lalu fallback ke
/// SearXNG dan DuckDuckGo. Kredensial Google hanya dibaca dari environment.
pub async fn search(http: &reqwest::Client, searx: &str, q: &str) -> Result<Vec<Hit>> {
    let google_key = std::env::var("GOOGLE_CSE_API_KEY").ok();
    let google_cx = std::env::var("GOOGLE_CSE_CX").ok();
    let mut errors = Vec::new();

    if let (Some(key), Some(cx)) = (google_key, google_cx) {
        match google_cse(http, q, &key, &cx).await {
            Ok(hits) if !hits.is_empty() => return Ok(hits),
            Ok(_) => {}
            Err(error) => errors.push(error.to_string()),
        }
    }

    if let Some(key) = crate::koneksi::brave_key() {
        match brave(http, q, &key).await {
            Ok(hits) if !hits.is_empty() => return Ok(hits),
            Ok(_) => {}
            Err(error) => errors.push(format!("Brave: {error}")),
        }
    }

    if !searx.trim().is_empty() {
        match searxng(http, searx, q).await {
            Ok(hits) if !hits.is_empty() => return Ok(hits),
            Ok(_) => {}
            Err(error) => errors.push(format!("SearXNG: {error}")),
        }
    }

    // DuckDuckGo diblokir di sebagian jaringan: beri waktu 8 detik, lalu jatuh ke Wikipedia (API resmi, tanpa key).
    // Setelah gagal, DuckDuckGo dilewati 10 menit agar tiap pencarian tidak menunggu 8 detik lagi.
    static DDG_GAGAL: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
    let lewati = DDG_GAGAL.lock().ok().and_then(|g| *g).is_some_and(|t| t.elapsed() < std::time::Duration::from_secs(600));
    if !lewati {
        match tokio::time::timeout(std::time::Duration::from_secs(8), ddg(http, q)).await {
            Ok(Ok(hits)) => {
                let hits = bersih(hits);
                if !hits.is_empty() {
                    return Ok(hits);
                }
            }
            Ok(Err(error)) => {
                errors.push(format!("DuckDuckGo: {error}"));
                if let Ok(mut g) = DDG_GAGAL.lock() {
                    *g = Some(std::time::Instant::now());
                }
            }
            Err(_) => {
                errors.push("DuckDuckGo: waktu habis".into());
                if let Ok(mut g) = DDG_GAGAL.lock() {
                    *g = Some(std::time::Instant::now());
                }
            }
        }
    }
    for lang in ["id", "en"] {
        match wikipedia(http, lang, q).await {
            Ok(hits) if !hits.is_empty() => return Ok(hits),
            Ok(_) => {}
            Err(error) => errors.push(format!("Wikipedia {lang}: {error}")),
        }
    }
    if errors.is_empty() {
        Ok(Vec::new())
    } else {
        anyhow::bail!("Semua penyedia pencarian gagal: {}", errors.join("; "))
    }
}

/// Buang hasil spam/dewasa/judi (mesin pencari publik kadang menyajikannya) sebelum dibaca atau ditampilkan.
pub fn bersih(hits: Vec<Hit>) -> Vec<Hit> {
    const BURUK: &[&str] = &["bokep", "porn", "xxx", "jav ", " jav", "sex", "ngentot", "hentai", "judi", "slot gacor", "togel", "casino", "bocil", "vcs", "telanjang"];
    hits.into_iter()
        .filter(|h| {
            let t = format!("{} {} {}", h.title, h.url, h.snippet).to_lowercase();
            !BURUK.iter().any(|b| t.contains(b))
        })
        .collect()
}

/// Brave Search API (key gratis di brave.com/search/api; isi env BRAVE_API_KEY): hasil web aman dengan SafeSearch ketat.
async fn brave(http: &reqwest::Client, q: &str, key: &str) -> Result<Vec<Hit>> {
    let v: serde_json::Value = http
        .get("https://api.search.brave.com/res/v1/web/search")
        .query(&[("q", q), ("count", "6"), ("safesearch", "strict"), ("country", "ID"), ("search_lang", "id")])
        .header("X-Subscription-Token", key)
        .header("Accept", "application/json")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(bersih(
        v["web"]["results"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|r| {
                        Some(Hit { title: strip_tags(r["title"].as_str()?), url: r["url"].as_str()?.to_string(), snippet: strip_tags(r["description"].as_str().unwrap_or("")) })
                    })
                    .collect()
            })
            .unwrap_or_default(),
    ))
}

/// Wikipedia (API resmi): pencarian teks penuh, hasil berupa artikel dengan cuplikan.
async fn wikipedia(http: &reqwest::Client, lang: &str, q: &str) -> Result<Vec<Hit>> {
    let v: serde_json::Value = http
        .get(format!("https://{lang}.wikipedia.org/w/api.php"))
        .query(&[("action", "query"), ("list", "search"), ("srsearch", q), ("srlimit", "4"), ("format", "json")])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(v["query"]["search"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|r| {
                    let t = r["title"].as_str()?;
                    Some(Hit { title: format!("{t} - Wikipedia"), url: format!("https://{lang}.wikipedia.org/wiki/{}", urlencoding::encode(&t.replace(' ', "_"))), snippet: strip_tags(r["snippet"].as_str().unwrap_or("")) })
                })
                .collect()
        })
        .unwrap_or_default())
}

async fn google_cse(
    http: &reqwest::Client,
    q: &str,
    key: &str,
    cx: &str,
) -> Result<Vec<Hit>> {
    let response = http
        .get("https://www.googleapis.com/customsearch/v1")
        .query(&[("key", key), ("cx", cx), ("q", q), ("num", "10")])
        .send()
        .await
        .map_err(|error| anyhow::anyhow!("Google CSE request failed: {}", error.without_url()))?;
    let status = response.status();
    if !status.is_success() {
        anyhow::bail!("Google CSE returned HTTP {}", status.as_u16());
    }
    let data: Value = response.json().await?;
    Ok(parse_google_cse(&data))
}

async fn searxng(http: &reqwest::Client, base: &str, q: &str) -> Result<Vec<Hit>> {
    let url = format!("{}/search?q={}&format=json", base.trim_end_matches('/'), urlencoding::encode(q));
    let v: Value = http.get(url).send().await?.error_for_status()?.json().await?;
    Ok(v["results"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|r| Hit {
                    title: r["title"].as_str().unwrap_or("").to_string(),
                    url: r["url"].as_str().unwrap_or("").to_string(),
                    snippet: r["content"].as_str().unwrap_or("").to_string(),
                })
                .filter(|h| h.url.starts_with("http"))
                .collect()
        })
        .unwrap_or_default())
}

async fn ddg(http: &reqwest::Client, q: &str) -> Result<Vec<Hit>> {
    let url = format!("https://html.duckduckgo.com/html/?q={}", urlencoding::encode(q));
    let html = http.get(url).send().await?.error_for_status()?.text().await?;
    Ok(parse_ddg(&html))
}

fn between<'a>(s: &'a str, a: &str, b: &str) -> Option<&'a str> {
    let i = s.find(a)? + a.len();
    let j = s[i..].find(b)?;
    Some(&s[i..i + j])
}

fn resolve(href: &str) -> String {
    let h = href.replace("&amp;", "&");
    if let Some(i) = h.find("uddg=") {
        let r = &h[i + 5..];
        let end = r.find('&').unwrap_or(r.len());
        urlencoding::decode(&r[..end]).map(|c| c.into_owned()).unwrap_or_default()
    } else if h.starts_with("//") {
        format!("https:{h}")
    } else {
        h
    }
}

pub fn parse_ddg(html: &str) -> Vec<Hit> {
    let mut hits = vec![];
    for chunk in html.split("class=\"result__a\"").skip(1) {
        let href = between(chunk, "href=\"", "\"").unwrap_or("");
        let title = chunk
            .find('>')
            .and_then(|i| chunk[i + 1..].find("</a>").map(|j| strip_tags(&chunk[i + 1..i + 1 + j])))
            .unwrap_or_default();
        let snippet = chunk
            .find("result__snippet")
            .and_then(|i| {
                let rest = &chunk[i..];
                let s = rest.find('>')?;
                let e = rest[s + 1..].find("</a>")?;
                Some(strip_tags(&rest[s + 1..s + 1 + e]))
            })
            .unwrap_or_default();
        let url = resolve(href);
        if url.starts_with("http") && !url.contains("duckduckgo.com/y.js") && !title.is_empty() {
            hits.push(Hit { title, url, snippet });
        }
    }
    hits
}

pub async fn fetch_text(http: &reqwest::Client, url: &str, max_chars: usize) -> Result<String> {
    let body = http.get(url).send().await?.error_for_status()?.text().await?;
    let mut s: String = body.chars().take(300_000).collect();
    for tag in ["script", "style", "noscript", "svg", "nav", "footer"] {
        s = remove_block(&s, tag);
    }
    Ok(truncate(&strip_tags(&s), max_chars))
}

fn remove_block(s: &str, tag: &str) -> String {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = String::new();
    let mut rest = s.to_string();
    loop {
        let low = rest.to_ascii_lowercase();
        match low.find(&open) {
            Some(i) => {
                out.push_str(&rest[..i]);
                match low[i..].find(&close) {
                    Some(j) => rest = rest[i + j + close.len()..].to_string(),
                    None => break,
                }
            }
            None => {
                out.push_str(&rest);
                break;
            }
        }
    }
    out
}

pub fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    let out = out
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ");
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

fn parse_google_cse(data: &Value) -> Vec<Hit> {
    data["items"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let url = item["link"].as_str()?;
                    url.starts_with("https://").then(|| Hit {
                        title: item["title"].as_str().unwrap_or("").to_string(),
                        url: url.to_string(),
                        snippet: item["snippet"].as_str().unwrap_or("").to_string(),
                    })
                })
                .filter(|hit| !hit.title.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #[test]
    fn bersih_membuang_spam_dewasa() {
        let h = |t: &str| Hit { title: t.into(), url: "https://x.id".into(), snippet: String::new() };
        let r = super::bersih(vec![h("Relay overload termal"), h("Koleksi bokep terbaru"), h("Slot gacor hari ini")]);
        assert_eq!(r.len(), 1);
    }


    use super::*;

    #[test]
    fn parse_ddg_extracts_hits() {
        let html = r#"<div class="result"><a rel="nofollow" class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fa%3Fx%3D1&amp;rut=abc">Judul <b>Satu</b></a>
        <a class="result__snippet" href="x">Ringkasan &amp; isi</a></div>
        <a rel="nofollow" class="result__a" href="https://duckduckgo.com/y.js?ad=1">Iklan</a>"#;
        let hits = parse_ddg(html);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].url, "https://example.com/a?x=1");
        assert_eq!(hits[0].title, "Judul Satu");
        assert_eq!(hits[0].snippet, "Ringkasan & isi");
    }

    #[test]
    fn parses_google_cse_results_and_drops_non_https_links() {
        let response: Value = serde_json::from_str(
            r#"{"items":[
                {"title":"Secure result","link":"https://example.com","snippet":"Useful source"},
                {"title":"Insecure result","link":"http://example.com","snippet":"Skip"}
            ]}"#,
        )
        .unwrap();
        let hits = parse_google_cse(&response);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Secure result");
        assert_eq!(hits[0].url, "https://example.com");
    }

    #[test]
    fn strips_blocks() {
        let s = remove_block("a<script>bad()</script>b<style>x{}</style>c", "script");
        assert_eq!(strip_tags(&s).replace(' ', ""), "ab" .to_string() + "x{}c");
    }

    #[test]
    fn auto_research_only_triggers_for_explicit_freshness_or_sources() {
        assert!(should_auto_research("Cari berita terbaru tentang cuaca"));
        assert!(should_auto_research("what is the latest version?"));
        assert!(!should_auto_research("Jelaskan cara kerja router"));
    }

    #[test]
    fn image_search_requires_clear_reuse_license() {
        assert!(reusable_license("CC BY-SA 4.0"));
        assert!(reusable_license("Public domain"));
        assert!(!reusable_license("CC BY-NC 4.0"));
        assert!(!reusable_license("All rights reserved"));
    }
}
