//! Ekstensi browser NOVA (Manifest V3, Edge/Chrome): memblokir iklan DI DALAM halaman (aturan jaringan dari daftar iklan NOVA +
//! penyembunyi elemen iklan + penjaga pop-up `window.open`) dan melapor ke NOVA agar peliharaan menembaknya. DNS saja tidak
//! bisa melihat isi halaman; ekstensi inilah yang menutup celah itu. Dibangkitkan ke folder data NOVA; dipasang manual sekali
//! lewat edge://extensions (Mode pengembang -> Muat yang belum dipaketkan), karena browser melarang pemasangan senyap.
use std::path::PathBuf;

pub fn dir() -> PathBuf {
    crate::config::data_dir().join("ekstensi")
}

const MAX_DOMAINS: usize = 120_000;
const CHUNK: usize = 1000;

const CONTENT: &str = r#"// NOVA: sembunyikan elemen iklan umum + laporkan pop-up yang ditahan.
const SEL = [
  'ins.adsbygoogle', '[id^="google_ads_"]', '[id^="div-gpt-ad"]', '[class*="adsbygoogle"]', 'iframe[src*="doubleclick"]',
  'iframe[src*="googlesyndication"]', 'iframe[src*="adservice"]', '[class*="popup-ad"]', '[class*="ad-banner"]',
  '[id*="ad-container"]', '[data-ad]', '[data-ad-slot]', 'a[href*="doubleclick.net"]', '.sponsored-ad', '#taboola-below-article'
].join(',');
const st = document.createElement('style');
st.textContent = SEL + '{display:none!important;visibility:hidden!important}';
(document.head || document.documentElement).appendChild(st);
window.addEventListener('message', (e) => {
  if (e.source === window && e.data && e.data.nova === 'popup') {
    try { chrome.runtime.sendMessage({ nova: 'popup', host: location.hostname }); } catch (_) {}
  }
});
"#;

const GUARD: &str = r#"// NOVA (dunia halaman): pop-up window.open tanpa interaksi pengguna ditahan.
(() => {
  const real = window.open;
  window.open = function (...a) {
    const ok = navigator.userActivation && navigator.userActivation.isActive;
    if (!ok) { window.postMessage({ nova: 'popup' }, '*'); return null; }
    return real.apply(this, a);
  };
})();
"#;

fn background(port: u16, token: &str) -> String {
    format!(
        r#"// NOVA: laporkan iklan yang diblokir ke aplikasi lokal agar peliharaan menembaknya.
const API = 'http://127.0.0.1:{port}/api/ad', TOKEN = '{token}';
let last = 0;
function report(host) {{
  const now = Date.now();
  if (now - last < 1500) return;
  last = now;
  fetch(API, {{ method: 'POST', headers: {{ 'x-nova-token': TOKEN, 'content-type': 'application/json' }}, body: JSON.stringify({{ host }}) }}).catch(() => {{}});
}}
if (chrome.declarativeNetRequest.onRuleMatchedDebug) {{
  chrome.declarativeNetRequest.onRuleMatchedDebug.addListener((i) => {{
    try {{ report(new URL(i.request.url).hostname); }} catch (_) {{}}
  }});
}}
chrome.runtime.onMessage.addListener((m) => {{ if (m && m.nova === 'popup') report(m.host + ' (popup)'); }});
"#
    )
}

fn domains() -> Vec<String> {
    let Ok(c) = rusqlite::Connection::open(crate::config::data_dir().join("iklan.db")) else { return vec![] };
    let Ok(mut st) = c.prepare("SELECT d FROM block LIMIT ?1") else { return vec![] };
    st.query_map([MAX_DOMAINS as i64], |r| r.get::<_, String>(0)).map(|r| r.flatten().collect()).unwrap_or_default()
}

/// Tulis seluruh ekstensi. Mengembalikan (folder, jumlah domain).
pub fn generate(port: u16, token: &str) -> std::io::Result<(PathBuf, usize)> {
    let d = dir();
    std::fs::create_dir_all(&d)?;
    let doms = domains();
    let rules: Vec<serde_json::Value> = doms
        .chunks(CHUNK)
        .enumerate()
        .map(|(i, ch)| {
            serde_json::json!({
                "id": i + 1, "priority": 1,
                "action": {"type": "block"},
                "condition": {"requestDomains": ch, "domainType": "thirdParty",
                    "resourceTypes": ["script","sub_frame","image","xmlhttprequest","ping","media","other","stylesheet"]}
            })
        })
        .collect();
    std::fs::write(d.join("rules.json"), serde_json::to_string(&rules)?)?;
    let manifest = serde_json::json!({
        "manifest_version": 3, "name": "NOVA Pemblokir Iklan", "version": "1.0.0",
        "description": "Pelengkap NOVA: blokir iklan & pop-up di dalam halaman, lapor ke peliharaan.",
        "permissions": ["declarativeNetRequest", "declarativeNetRequestFeedback"],
        "host_permissions": ["<all_urls>"],
        "background": {"service_worker": "background.js"},
        "declarative_net_request": {"rule_resources": [{"id": "nova", "enabled": true, "path": "rules.json"}]},
        "content_scripts": [
            {"matches": ["<all_urls>"], "js": ["guard.js"], "run_at": "document_start", "world": "MAIN", "all_frames": true},
            {"matches": ["<all_urls>"], "js": ["content.js"], "run_at": "document_start", "all_frames": true}
        ]
    });
    std::fs::write(d.join("manifest.json"), serde_json::to_string_pretty(&manifest)?)?;
    std::fs::write(d.join("content.js"), CONTENT)?;
    std::fs::write(d.join("guard.js"), GUARD)?;
    std::fs::write(d.join("background.js"), background(port, token))?;
    Ok((d, doms.len()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn background_embeds_port_and_token() {
        let b = background(7878, "abc");
        assert!(b.contains("127.0.0.1:7878/api/ad") && b.contains("'abc'"));
    }
}
