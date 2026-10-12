//! Baca halaman yang dirender Chrome (JavaScript dijalankan) lewat `--headless --dump-dom`, tanpa dependensi tambahan.
//! Hanya membaca; klik/ketik memerlukan CDP (WebSocket) dan belum ada.
use std::process::Stdio;
use std::time::Duration;

pub(crate) fn chrome() -> Option<&'static str> {
    ["C:/Program Files/Google/Chrome/Application/chrome.exe", "C:/Program Files (x86)/Google/Chrome/Application/chrome.exe", "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe"]
        .into_iter()
        .find(|p| std::path::Path::new(p).exists())
}

/// HTML → teks polos: buang script/style, tag, dan rapikan spasi.
pub fn teks(html: &str) -> String {
    let drop = regex::Regex::new(r"(?is)<script\b.*?</script>|<style\b.*?</style>|<noscript\b.*?</noscript>|<svg\b.*?</svg>").unwrap();
    let tag = regex::Regex::new(r"(?s)<[^>]*>").unwrap();
    let a = drop.replace_all(html, " ");
    let t = tag.replace_all(&a, " ");
    let t = t.replace("&nbsp;", " ").replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"");
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub async fn baca(url: &str) -> String {
    let url = url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) || url.contains(['"', ' ']) {
        return "Beri URL http(s) yang valid, mis. 'baca dengan chrome https://example.com'.".into();
    }
    let Some(exe) = chrome() else { return "Chrome/Edge tidak ditemukan.".into() };
    let prof = std::env::temp_dir().join("nova-chrome");
    let mut cmd = tokio::process::Command::new(exe);
    cmd.args(["--headless=new", "--disable-gpu", "--no-first-run", "--virtual-time-budget=8000", "--dump-dom"])
        .arg(format!("--user-data-dir={}", prof.display()))
        .arg(url)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    match tokio::time::timeout(Duration::from_secs(30), cmd.output()).await {
        Ok(Ok(o)) if !o.stdout.is_empty() => {
            let t = teks(&String::from_utf8_lossy(&o.stdout));
            crate::research::truncate(&t, 3000)
        }
        Ok(_) => "Chrome tidak mengembalikan isi halaman.".into(),
        Err(_) => "Chrome terlalu lama (30 dtk), dibatalkan.".into(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn strips_markup() {
        assert_eq!(super::teks("<html><style>a{}</style><p>Halo&nbsp;<b>dunia</b></p><script>x()</script></html>"), "Halo dunia");
    }
}
