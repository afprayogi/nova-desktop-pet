//! Integrasi Google MULTI-AKUN (Gmail + Kalender, HANYA-BACA): OAuth 2.0 loopback + PKCE lewat browser Bos; setiap akun ditambah
//! dengan jendela pemilih akun Google. Token dienkripsi DPAPI (secure.rs) dan disimpan per akun di google_accounts.json.
//! Yang dibaca: pengirim + subjek email belum dibaca (bukan isi email) dan acara kalender 24 jam ke depan.
use crate::config::data_dir;
use crate::engine::Tx;
use base64::Engine as _;
use chrono::{DateTime, Local, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const SCOPES: &str = "https://www.googleapis.com/auth/gmail.readonly https://www.googleapis.com/auth/calendar.readonly";

pub const SETUP_HELP: &str = "Untuk menghubungkan Google: (1) buka console.cloud.google.com, buat proyek, aktifkan Gmail API dan Google Calendar API. \
(2) OAuth consent screen: tipe External, mode Testing, tambahkan SEMUA email Bos yang mau dihubungkan sebagai test user. (3) Credentials > Create OAuth client ID > jenis 'Desktop app'. \
(4) Salin Client ID dan Client secret ke file google.json di folder data NOVA: {\"client_id\":\"...\",\"client_secret\":\"...\"}. (5) Ketik /google lagi; ulangi /google untuk tiap akun tambahan.";

#[derive(Deserialize)]
struct Creds {
    client_id: String,
    client_secret: String,
}

#[derive(Serialize, Deserialize, Default, Clone)]
struct Token {
    refresh_token: String,
    #[serde(default)]
    access_token: String,
    #[serde(default)]
    expires_at: i64,
}

#[derive(Serialize, Deserialize, Clone)]
struct Acct {
    email: String,
    /// Token (JSON) terenkripsi DPAPI.
    enc: String,
}

fn creds() -> Option<Creds> {
    serde_json::from_str(&std::fs::read_to_string(data_dir().join("google.json")).ok()?).ok()
}

fn accounts_file() -> std::path::PathBuf {
    data_dir().join("google_accounts.json")
}

fn load_accounts() -> Vec<Acct> {
    std::fs::read_to_string(accounts_file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save_accounts(v: &[Acct]) {
    let _ = std::fs::write(accounts_file(), serde_json::to_string_pretty(v).unwrap_or_default());
}

fn token_of(a: &Acct) -> Option<Token> {
    serde_json::from_str(&crate::secure::unprotect(&a.enc)?).ok()
}

fn put_token(email: &str, t: &Token) -> bool {
    let Some(enc) = crate::secure::protect(&serde_json::to_string(t).unwrap_or_default()) else { return false };
    let mut v = load_accounts();
    v.retain(|a| a.email != email);
    v.push(Acct { email: email.to_string(), enc });
    save_accounts(&v);
    true
}

pub fn emails() -> Vec<String> {
    load_accounts().into_iter().map(|a| a.email).collect()
}

pub fn connected() -> bool {
    !load_accounts().is_empty()
}

pub fn disconnect(which: &str) -> String {
    let mut v = load_accounts();
    let n = v.len();
    if which == "semua" || which.is_empty() {
        v.clear();
    } else {
        v.retain(|a| a.email != which);
    }
    save_accounts(&v);
    if v.is_empty() {
        crate::konteks::set_google_events(vec![]);
    }
    format!("🔑 {} akun Google diputus; {} tersisa. (Cabut izin sepenuhnya di myaccount.google.com/permissions bila perlu.)", n - v.len(), v.len())
}

pub(crate) fn b64url(b: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
}

pub fn pkce_challenge(verifier: &str) -> String {
    b64url(&Sha256::digest(verifier.as_bytes()))
}

pub(crate) fn rand_str(n: usize) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
    let mut r = rand::thread_rng();
    (0..n).map(|_| A[r.gen_range(0..A.len())] as char).collect()
}

pub(crate) fn form(pairs: &[(&str, &str)]) -> String {
    pairs.iter().map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v))).collect::<Vec<_>>().join("&")
}

async fn token_request(http: &reqwest::Client, body: String) -> Result<serde_json::Value, String> {
    let r = http.post("https://oauth2.googleapis.com/token").header("Content-Type", "application/x-www-form-urlencoded").body(body).send().await.map_err(|e| e.to_string())?;
    let ok = r.status().is_success();
    let v: serde_json::Value = r.json().await.map_err(|e| e.to_string())?;
    if ok { Ok(v) } else { Err(v["error_description"].as_str().or(v["error"].as_str()).unwrap_or("gagal").to_string()) }
}

/// Buka URL di browser bawaan Bos.
pub fn open_in_browser(url: &str) {
    let _ = std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).spawn();
}

/// Tambah satu akun: jendela browser dengan pemilih akun Google. Dapat dipanggil berulang untuk akun lain.
pub async fn connect(tx: Tx, http: reqwest::Client) {
    let Some(c) = creds() else {
        tx.sys(format!("🔑 {SETUP_HELP}"));
        return;
    };
    let Ok(listener) = tokio::net::TcpListener::bind("127.0.0.1:0").await else { return };
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
    let redirect = format!("http://127.0.0.1:{port}");
    let (verifier, state) = (rand_str(64), rand_str(16));
    let url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?{}",
        form(&[
            ("client_id", &c.client_id), ("redirect_uri", &redirect), ("response_type", "code"), ("scope", SCOPES),
            ("code_challenge", &pkce_challenge(&verifier)), ("code_challenge_method", "S256"), ("access_type", "offline"), ("prompt", "select_account consent"), ("state", &state),
        ])
    );
    open_in_browser(&url);
    tx.sys("🔑 Browser dibuka: PILIH akun Google yang mau ditambahkan lalu setujui akses BACA Gmail & Kalender (batas 3 menit).");
    let code = match tokio::time::timeout(Duration::from_secs(180), async {
        let (mut s, _) = listener.accept().await.ok()?;
        let mut buf = vec![0u8; 4096];
        let n = s.read(&mut buf).await.ok()?;
        let req = String::from_utf8_lossy(&buf[..n]).to_string();
        let q = req.lines().next()?.split_whitespace().nth(1)?.split_once('?')?.1.to_string();
        let get = |k: &str| q.split('&').find_map(|p| p.strip_prefix(&format!("{k}=")).map(|v| urlencoding::decode(v).map(|d| d.into_owned()).unwrap_or_default()));
        let ok = get("state").as_deref() == Some(state.as_str());
        let body = if ok { "NOVA: akun berhasil terhubung. Boleh menutup tab ini." } else { "NOVA: permintaan tidak valid." };
        let _ = s.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\n\r\n{body}", body.len()).as_bytes()).await;
        if ok { get("code") } else { None }
    })
    .await
    {
        Ok(Some(c)) => c,
        _ => {
            tx.sys("🔑 Persetujuan Google tidak selesai. Ketik /google untuk mencoba lagi.");
            return;
        }
    };
    let body = form(&[("code", &code), ("client_id", &c.client_id), ("client_secret", &c.client_secret), ("redirect_uri", &redirect), ("grant_type", "authorization_code"), ("code_verifier", &verifier)]);
    match token_request(&http, body).await {
        Ok(v) => {
            let t = Token {
                refresh_token: v["refresh_token"].as_str().unwrap_or("").to_string(),
                access_token: v["access_token"].as_str().unwrap_or("").to_string(),
                expires_at: Local::now().timestamp() + v["expires_in"].as_i64().unwrap_or(3000),
            };
            if t.refresh_token.is_empty() {
                tx.sys("🔑 Google tidak mengirim refresh token. Cabut izin NOVA di myaccount.google.com/permissions lalu ketik /google lagi.");
                return;
            }
            // Ambil alamat email akun dari profil Gmail (tidak butuh izin tambahan).
            let email = match http.get("https://gmail.googleapis.com/gmail/v1/users/me/profile").bearer_auth(&t.access_token).send().await {
                Ok(r) => r.json::<serde_json::Value>().await.ok().and_then(|j| j["emailAddress"].as_str().map(String::from)),
                Err(_) => None,
            };
            let Some(email) = email else {
                tx.sys("🔑 Tidak bisa membaca alamat email akun (cek Gmail API aktif di proyekmu).");
                return;
            };
            if !put_token(&email, &t) {
                tx.sys("🔑 Gagal mengenkripsi token (PowerShell/DPAPI tidak tersedia).");
                return;
            }
            tx.sys(format!("🔑 Akun {email} terhubung (hanya-baca, token terenkripsi). Total {} akun. Tambah lagi: /google", emails().len()));
            poll(&tx, &http, true).await;
        }
        Err(e) => tx.sys(format!("🔑 Gagal menukar kode: {e}")),
    }
}

async fn access(http: &reqwest::Client, a: &Acct) -> Option<String> {
    let mut t = token_of(a)?;
    if t.expires_at > Local::now().timestamp() + 60 && !t.access_token.is_empty() {
        return Some(t.access_token);
    }
    let c = creds()?;
    let v = token_request(http, form(&[("client_id", &c.client_id), ("client_secret", &c.client_secret), ("refresh_token", &t.refresh_token), ("grant_type", "refresh_token")])).await.ok()?;
    t.access_token = v["access_token"].as_str()?.to_string();
    t.expires_at = Local::now().timestamp() + v["expires_in"].as_i64().unwrap_or(3000);
    put_token(&a.email, &t);
    Some(t.access_token)
}

pub fn parse_from_subject(v: &serde_json::Value) -> (String, String) {
    let h = |name: &str| v["payload"]["headers"].as_array().and_then(|a| a.iter().find(|x| x["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(name)))).and_then(|x| x["value"].as_str()).unwrap_or("").to_string();
    let from = h("From");
    let from = from.split('<').next().unwrap_or(&from).trim().trim_matches('"').to_string();
    (from, h("Subject"))
}

pub fn parse_events(v: &serde_json::Value) -> Vec<(String, crate::konteks::Ev)> {
    v["items"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|e| {
                    let start = DateTime::parse_from_rfc3339(e["start"]["dateTime"].as_str()?).ok()?.with_timezone(&Local);
                    Some((e["id"].as_str()?.to_string(), crate::konteks::Ev { start, title: e["summary"].as_str().unwrap_or("(tanpa judul)").chars().take(80).collect() }))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn seen(c: &rusqlite::Connection, key: &str) -> bool {
    c.execute("INSERT OR IGNORE INTO google_seen VALUES(?1)", [key]).map(|n| n == 0).unwrap_or(true)
}

async fn get_json(http: &reqwest::Client, tok: &str, url: String) -> Option<serde_json::Value> {
    http.get(url).bearer_auth(tok).send().await.ok()?.json::<serde_json::Value>().await.ok()
}

/// Email belum dibaca satu akun (untuk perintah "cek email"): daftar (dari, subjek).
pub async fn unread_now(http: &reqwest::Client, a: &str) -> Vec<(String, String)> {
    let Some(acct) = load_accounts().into_iter().find(|x| x.email == a) else { return vec![] };
    let Some(tok) = access(http, &acct).await else { return vec![] };
    let Some(list) = get_json(http, &tok, "https://gmail.googleapis.com/gmail/v1/users/me/messages?q=is%3Aunread%20in%3Ainbox&maxResults=6".into()).await else { return vec![] };
    let mut out = vec![];
    for m in list["messages"].as_array().cloned().unwrap_or_default() {
        if let Some(id) = m["id"].as_str() {
            if let Some(d) = get_json(http, &tok, format!("https://gmail.googleapis.com/gmail/v1/users/me/messages/{id}?format=metadata&metadataHeaders=From&metadataHeaders=Subject")).await {
                out.push(parse_from_subject(&d));
            }
        }
    }
    out
}

/// Satu putaran untuk SEMUA akun: email belum dibaca + kalender. `baseline` = tandai yang sudah ada tanpa memberi tahu.
pub async fn poll(tx: &Tx, http: &reqwest::Client, baseline_new_only: bool) {
    let Ok(c) = crate::assistant::db() else { return };
    let _ = c.execute_batch("CREATE TABLE IF NOT EXISTS google_seen(id TEXT PRIMARY KEY)");
    let mut all_events: Vec<crate::konteks::Ev> = vec![];
    for acct in load_accounts() {
        let Some(tok) = access(http, &acct).await else { continue };
        let first = c.query_row("SELECT 1 FROM google_seen WHERE id=?1", [format!("base:{}", acct.email)], |_| Ok(())).is_err();
        let baseline = first || baseline_new_only && first;
        if first {
            let _ = c.execute("INSERT OR IGNORE INTO google_seen VALUES(?1)", [format!("base:{}", acct.email)]);
        }
        // Gmail
        if let Some(list) = get_json(http, &tok, "https://gmail.googleapis.com/gmail/v1/users/me/messages?q=is%3Aunread%20in%3Ainbox%20newer_than%3A2d&maxResults=8".into()).await {
            let mut new = vec![];
            for m in list["messages"].as_array().cloned().unwrap_or_default() {
                let Some(id) = m["id"].as_str() else { continue };
                if seen(&c, &format!("m{}:{id}", acct.email)) || baseline {
                    continue;
                }
                if let Some(d) = get_json(http, &tok, format!("https://gmail.googleapis.com/gmail/v1/users/me/messages/{id}?format=metadata&metadataHeaders=From&metadataHeaders=Subject")).await {
                    new.push(parse_from_subject(&d));
                }
            }
            if !new.is_empty() {
                let mut lines: Vec<(usize, String)> = new.iter().take(3).map(|(f, s)| (3, format!("📧 [{}] {f}: {}", acct.email, s.chars().take(80).collect::<String>()))).collect();
                if new.len() > 3 {
                    lines.push((3, format!("…dan {} email baru lainnya di {}.", new.len() - 3, acct.email)));
                }
                crate::notify::toast("NOVA - email baru", &lines[0].1);
                crate::adegan::play(tx, lines);
            }
        }
        // Kalender 24 jam ke depan
        let (now, end) = (Utc::now(), Utc::now() + chrono::Duration::hours(24));
        let fmt = |t: DateTime<Utc>| urlencoding::encode(&t.format("%Y-%m-%dT%H:%M:%SZ").to_string()).into_owned();
        if let Some(v) = get_json(http, &tok, format!("https://www.googleapis.com/calendar/v3/calendars/primary/events?timeMin={}&timeMax={}&singleEvents=true&orderBy=startTime&maxResults=15", fmt(now), fmt(end))).await {
            let mut fresh = vec![];
            for (id, ev) in parse_events(&v) {
                if !seen(&c, &format!("e{}:{id}", acct.email)) && !baseline {
                    fresh.push(format!("📅 [{}] Acara baru: {} ({})", acct.email, ev.title, ev.start.format("%d/%m %H:%M")));
                }
                all_events.push(ev);
            }
            if !fresh.is_empty() {
                crate::adegan::play(tx, fresh.into_iter().take(3).map(|t| (3, t)).collect());
            }
        }
    }
    crate::konteks::set_google_events(all_events);
}

/// Pindahkan token lama (google_token.json, teks biasa) ke penyimpanan terenkripsi multi-akun.
async fn migrate_legacy(tx: &Tx, http: &reqwest::Client) {
    let legacy = data_dir().join("google_token.json");
    let Ok(txt) = std::fs::read_to_string(&legacy) else { return };
    let Ok(mut t) = serde_json::from_str::<Token>(&txt) else { return };
    let Some(c) = creds() else { return };
    let Ok(v) = token_request(http, form(&[("client_id", &c.client_id), ("client_secret", &c.client_secret), ("refresh_token", &t.refresh_token), ("grant_type", "refresh_token")])).await else { return };
    t.access_token = v["access_token"].as_str().unwrap_or("").to_string();
    t.expires_at = Local::now().timestamp() + v["expires_in"].as_i64().unwrap_or(3000);
    let email = match http.get("https://gmail.googleapis.com/gmail/v1/users/me/profile").bearer_auth(&t.access_token).send().await {
        Ok(r) => r.json::<serde_json::Value>().await.ok().and_then(|j| j["emailAddress"].as_str().map(String::from)),
        Err(_) => None,
    };
    if let Some(e) = email {
        if put_token(&e, &t) {
            let _ = std::fs::remove_file(&legacy);
            tx.sys(format!("🔑 Token lama dipindahkan ke penyimpanan terenkripsi: {e}"));
        }
    }
}

/// Polling latar tiap 5 menit untuk semua akun (Google + IMAP).
pub async fn run(tx: Tx) {
    let http = crate::research::client(20);
    tokio::time::sleep(Duration::from_secs(25)).await;
    migrate_legacy(&tx, &http).await;
    loop {
        if connected() {
            poll(&tx, &http, false).await;
        }
        crate::outlook::poll(&tx, &http).await;
        crate::imap::poll(&tx).await;
        tokio::time::sleep(Duration::from_secs(300)).await;
    }
}

/// Ringkasan untuk perintah "cek email" / /email: semua akun + email belum dibaca.
pub async fn summary(http: &reqwest::Client) -> String {
    let mut s = String::new();
    let g = emails();
    let im = crate::imap::accounts();
    let ol = crate::outlook::emails();
    if g.is_empty() && im.is_empty() && ol.is_empty() {
        return "Belum ada akun email. Google: /google, Outlook/Microsoft 365: /outlook (keduanya lewat browser, bisa banyak akun). Yahoo/iCloud/domain sendiri: /email imap auto <alamat> <sandi-aplikasi>.".into();
    }
    for a in &g {
        let u = unread_now(http, a).await;
        s.push_str(&format!("📧 {a} (Google): {} belum dibaca\n", u.len()));
        for (f, subj) in u.iter().take(4) {
            s.push_str(&format!("   • {f}: {}\n", subj.chars().take(70).collect::<String>()));
        }
    }
    for a in &ol {
        let u = crate::outlook::unread_now(http, a).await;
        s.push_str(&format!("📧 {a} (Outlook): {} belum dibaca\n", u.len()));
        for (f, subj) in u.iter().take(4) {
            s.push_str(&format!("   • {f}: {}\n", subj.chars().take(70).collect::<String>()));
        }
    }
    for a in &im {
        match crate::imap::unread(a).await {
            Ok(u) => {
                s.push_str(&format!("📧 {} (IMAP): {} belum dibaca\n", a.label, u.len()));
                for (_, f, subj) in u.iter().take(4) {
                    s.push_str(&format!("   • {f}: {}\n", subj.chars().take(70).collect::<String>()));
                }
            }
            Err(e) => s.push_str(&format!("📧 {} (IMAP): gagal — {e}\n", a.label)),
        }
    }
    s.push_str("Buka kotak masuk di browser: /email buka <alamat>. Tambah akun: /google, /outlook, atau /email imap ...");
    s
}

/// Buka webmail di browser untuk akun tertentu.
pub fn open_inbox(who: &str) -> String {
    let l = who.to_lowercase();
    let url = if emails().iter().any(|e| e.to_lowercase() == l) {
        format!("https://mail.google.com/mail/?authuser={}", urlencoding::encode(who))
    } else if crate::outlook::emails().iter().any(|e| e.to_lowercase() == l) {
        "https://outlook.office.com/mail/".into()
    } else if l.ends_with("@yahoo.com") || l.ends_with("@yahoo.co.id") || l.ends_with("@ymail.com") {
        "https://mail.yahoo.com".into()
    } else if l.ends_with("@icloud.com") || l.ends_with("@me.com") {
        "https://www.icloud.com/mail".into()
    } else {
        return format!("Akun '{who}' tidak dikenal. Ketik /email untuk daftar akun.");
    };
    open_in_browser(&url);
    format!("🌐 Membuka kotak masuk {who} di browser.")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pkce_matches_rfc7636_example() {
        assert_eq!(pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }
    #[test]
    fn parses_gmail_and_calendar() {
        let m = serde_json::json!({"payload":{"headers":[{"name":"From","value":"\"Budi\" <budi@x.com>"},{"name":"Subject","value":"Rapat besok"}]}});
        assert_eq!(parse_from_subject(&m), ("Budi".to_string(), "Rapat besok".to_string()));
        let e = serde_json::json!({"items":[{"id":"a1","summary":"Review","start":{"dateTime":"2026-10-11T15:00:00+07:00"}},{"id":"a2","summary":"Libur","start":{"date":"2026-10-12"}}]});
        let p = parse_events(&e);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].1.title, "Review");
    }
    #[test]
    fn open_inbox_rejects_unknown() {
        assert!(open_inbox("tidak-ada@contoh.id").contains("tidak dikenal"));
    }
}
