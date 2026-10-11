//! Outlook.com / Hotmail / Microsoft 365 MULTI-AKUN (Mail + Kalender, HANYA-BACA) lewat Microsoft Graph: OAuth 2.0 klien publik
//! (tanpa client secret) dengan PKCE + loopback di browser Bos. Bos mendaftarkan aplikasinya sendiri (gratis) di portal Azure
//! dan menaruh client_id di outlook.json. Token dienkripsi DPAPI (secure.rs). Yang dibaca: pengirim + subjek email belum dibaca
//! dan acara kalender 24 jam ke depan.
use crate::config::data_dir;
use crate::engine::Tx;
use crate::google::{form, open_in_browser, pkce_challenge, rand_str};
use chrono::{DateTime, Local, NaiveDateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const SCOPE: &str = "offline_access Mail.Read Calendars.Read User.Read";

pub const SETUP_HELP: &str = "Untuk menghubungkan Outlook/Microsoft 365: (1) buka portal.azure.com > Microsoft Entra ID > App registrations > New registration. \
(2) Supported account types: 'Accounts in any organizational directory and personal Microsoft accounts'. (3) Redirect URI: platform 'Mobile and desktop applications', isi http://localhost. \
(4) API permissions > Microsoft Graph > Delegated: Mail.Read, Calendars.Read, User.Read, offline_access. (5) Salin 'Application (client) ID' ke file outlook.json di folder data NOVA: {\"client_id\":\"...\"} \
(opsional \"tenant\":\"common\"). (6) Ketik /outlook; ulangi untuk tiap akun tambahan.";

#[derive(Deserialize)]
struct Cfg {
    client_id: String,
    #[serde(default = "common")]
    tenant: String,
}

fn common() -> String {
    "common".into()
}

fn cfg() -> Option<Cfg> {
    serde_json::from_str(&std::fs::read_to_string(data_dir().join("outlook.json")).ok()?).ok()
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
    enc: String,
}

fn file() -> std::path::PathBuf {
    data_dir().join("outlook_accounts.json")
}

fn load() -> Vec<Acct> {
    std::fs::read_to_string(file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save(v: &[Acct]) {
    let _ = std::fs::write(file(), serde_json::to_string_pretty(v).unwrap_or_default());
}

fn put_token(email: &str, t: &Token) -> bool {
    let Some(enc) = crate::secure::protect(&serde_json::to_string(t).unwrap_or_default()) else { return false };
    let mut v = load();
    v.retain(|a| a.email != email);
    v.push(Acct { email: email.to_string(), enc });
    save(&v);
    true
}

pub fn emails() -> Vec<String> {
    load().into_iter().map(|a| a.email).collect()
}

pub fn disconnect(which: &str) -> String {
    let mut v = load();
    let n = v.len();
    if which == "semua" || which.is_empty() {
        v.clear();
    } else {
        v.retain(|a| a.email != which);
    }
    save(&v);
    if v.is_empty() {
        crate::konteks::set_outlook_events(vec![]);
    }
    format!("🔑 {} akun Outlook diputus; {} tersisa.", n - v.len(), v.len())
}

async fn token_request(http: &reqwest::Client, tenant: &str, body: String) -> Result<serde_json::Value, String> {
    let url = format!("https://login.microsoftonline.com/{tenant}/oauth2/v2.0/token");
    let r = http.post(url).header("Content-Type", "application/x-www-form-urlencoded").body(body).send().await.map_err(|e| e.to_string())?;
    let ok = r.status().is_success();
    let v: serde_json::Value = r.json().await.map_err(|e| e.to_string())?;
    if ok { Ok(v) } else { Err(v["error_description"].as_str().or(v["error"].as_str()).unwrap_or("gagal").lines().next().unwrap_or("gagal").to_string()) }
}

fn token_from(v: &serde_json::Value, old_refresh: &str) -> Token {
    Token {
        refresh_token: v["refresh_token"].as_str().unwrap_or(old_refresh).to_string(),
        access_token: v["access_token"].as_str().unwrap_or("").to_string(),
        expires_at: Local::now().timestamp() + v["expires_in"].as_i64().unwrap_or(3000),
    }
}

/// Tambah satu akun lewat browser (pemilih akun Microsoft). Dapat diulang untuk akun lain.
pub async fn connect(tx: Tx, http: reqwest::Client) {
    let Some(c) = cfg() else {
        tx.sys(format!("🔑 {SETUP_HELP}"));
        return;
    };
    let Ok(listener) = tokio::net::TcpListener::bind("127.0.0.1:0").await else { return };
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
    let redirect = format!("http://localhost:{port}");
    let (verifier, state) = (rand_str(64), rand_str(16));
    let url = format!(
        "https://login.microsoftonline.com/{}/oauth2/v2.0/authorize?{}",
        c.tenant,
        form(&[
            ("client_id", &c.client_id), ("response_type", "code"), ("redirect_uri", &redirect), ("response_mode", "query"), ("scope", SCOPE),
            ("state", &state), ("code_challenge", &pkce_challenge(&verifier)), ("code_challenge_method", "S256"), ("prompt", "select_account"),
        ])
    );
    open_in_browser(&url);
    tx.sys("🔑 Browser dibuka: PILIH akun Microsoft yang mau ditambahkan lalu setujui akses BACA email & kalender (batas 3 menit).");
    let code = match tokio::time::timeout(Duration::from_secs(180), async {
        let (mut s, _) = listener.accept().await.ok()?;
        let mut buf = vec![0u8; 4096];
        let n = s.read(&mut buf).await.ok()?;
        let req = String::from_utf8_lossy(&buf[..n]).to_string();
        let q = req.lines().next()?.split_whitespace().nth(1)?.split_once('?')?.1.to_string();
        let get = |k: &str| q.split('&').find_map(|p| p.strip_prefix(&format!("{k}=")).map(|v| urlencoding::decode(v).map(|d| d.into_owned()).unwrap_or_default()));
        let ok = get("state").as_deref() == Some(state.as_str());
        let body = if ok { "NOVA: akun Microsoft berhasil terhubung. Boleh menutup tab ini." } else { "NOVA: permintaan tidak valid." };
        let _ = s.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\n\r\n{body}", body.len()).as_bytes()).await;
        if ok { get("code") } else { None }
    })
    .await
    {
        Ok(Some(c)) => c,
        _ => {
            tx.sys("🔑 Persetujuan Microsoft tidak selesai. Ketik /outlook untuk mencoba lagi.");
            return;
        }
    };
    let body = form(&[("client_id", &c.client_id), ("scope", SCOPE), ("code", &code), ("redirect_uri", &redirect), ("grant_type", "authorization_code"), ("code_verifier", &verifier)]);
    match token_request(&http, &c.tenant, body).await {
        Ok(v) => {
            let t = token_from(&v, "");
            if t.refresh_token.is_empty() {
                tx.sys("🔑 Microsoft tidak mengirim refresh token (pastikan izin offline_access ditambahkan).");
                return;
            }
            let email = match http.get("https://graph.microsoft.com/v1.0/me?$select=mail,userPrincipalName").bearer_auth(&t.access_token).send().await {
                Ok(r) => r.json::<serde_json::Value>().await.ok().and_then(|j| j["mail"].as_str().or(j["userPrincipalName"].as_str()).map(String::from)),
                Err(_) => None,
            };
            let Some(email) = email else {
                tx.sys("🔑 Tidak bisa membaca profil akun (cek izin User.Read).");
                return;
            };
            if !put_token(&email, &t) {
                tx.sys("🔑 Gagal mengenkripsi token (PowerShell/DPAPI tidak tersedia).");
                return;
            }
            tx.sys(format!("🔑 Akun Outlook {email} terhubung (hanya-baca, token terenkripsi). Total {} akun Outlook.", emails().len()));
            poll(&tx, &http).await;
        }
        Err(e) => tx.sys(format!("🔑 Gagal menukar kode: {e}")),
    }
}

async fn access(http: &reqwest::Client, a: &Acct) -> Option<String> {
    let mut t: Token = serde_json::from_str(&crate::secure::unprotect(&a.enc)?).ok()?;
    if t.expires_at > Local::now().timestamp() + 60 && !t.access_token.is_empty() {
        return Some(t.access_token);
    }
    let c = cfg()?;
    let v = token_request(http, &c.tenant, form(&[("client_id", &c.client_id), ("scope", SCOPE), ("refresh_token", &t.refresh_token), ("grant_type", "refresh_token")])).await.ok()?;
    t = token_from(&v, &t.refresh_token);
    put_token(&a.email, &t);
    Some(t.access_token)
}

pub fn parse_messages(v: &serde_json::Value) -> Vec<(String, String, String)> {
    v["value"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|m| {
                    let id = m["id"].as_str()?.to_string();
                    let from = m["from"]["emailAddress"]["name"].as_str().or(m["from"]["emailAddress"]["address"].as_str()).unwrap_or("?").to_string();
                    Some((id, from, m["subject"].as_str().unwrap_or("(tanpa subjek)").to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn parse_events(v: &serde_json::Value) -> Vec<(String, crate::konteks::Ev)> {
    v["value"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|e| {
                    let raw = e["start"]["dateTime"].as_str()?;
                    let n = NaiveDateTime::parse_from_str(raw.split('.').next()?, "%Y-%m-%dT%H:%M:%S").ok()?;
                    let start = Utc.from_utc_datetime(&n).with_timezone(&Local);
                    Some((e["id"].as_str()?.to_string(), crate::konteks::Ev { start, title: e["subject"].as_str().unwrap_or("(tanpa judul)").chars().take(80).collect() }))
                })
                .collect()
        })
        .unwrap_or_default()
}

async fn get_json(http: &reqwest::Client, tok: &str, url: String, utc: bool) -> Option<serde_json::Value> {
    let mut r = http.get(url).bearer_auth(tok);
    if utc {
        r = r.header("Prefer", "outlook.timezone=\"UTC\"");
    }
    r.send().await.ok()?.json::<serde_json::Value>().await.ok()
}

const MAIL_URL: &str = "https://graph.microsoft.com/v1.0/me/mailFolders/inbox/messages?$filter=isRead%20eq%20false&$top=8&$select=id,subject,from";

/// Email belum dibaca untuk satu akun: (dari, subjek).
pub async fn unread_now(http: &reqwest::Client, email: &str) -> Vec<(String, String)> {
    let Some(a) = load().into_iter().find(|x| x.email == email) else { return vec![] };
    let Some(tok) = access(http, &a).await else { return vec![] };
    get_json(http, &tok, MAIL_URL.into(), false).await.map(|v| parse_messages(&v).into_iter().map(|m| (m.1, m.2)).collect()).unwrap_or_default()
}

fn seen(c: &rusqlite::Connection, key: &str) -> bool {
    c.execute("INSERT OR IGNORE INTO google_seen VALUES(?1)", [key]).map(|n| n == 0).unwrap_or(true)
}

/// Satu putaran untuk semua akun Outlook: email baru + kalender.
pub async fn poll(tx: &Tx, http: &reqwest::Client) {
    let Ok(c) = crate::assistant::db() else { return };
    let _ = c.execute_batch("CREATE TABLE IF NOT EXISTS google_seen(id TEXT PRIMARY KEY)");
    let mut all_events = vec![];
    for a in load() {
        let Some(tok) = access(http, &a).await else { continue };
        let first = c.query_row("SELECT 1 FROM google_seen WHERE id=?1", [format!("obase:{}", a.email)], |_| Ok(())).is_err();
        if first {
            let _ = c.execute("INSERT OR IGNORE INTO google_seen VALUES(?1)", [format!("obase:{}", a.email)]);
        }
        if let Some(v) = get_json(http, &tok, MAIL_URL.into(), false).await {
            let mut new = vec![];
            for (id, from, subj) in parse_messages(&v) {
                if !seen(&c, &format!("om{}:{id}", a.email)) && !first {
                    new.push((from, subj));
                }
            }
            if !new.is_empty() {
                let mut lines: Vec<(usize, String)> = new.iter().take(3).map(|(f, s)| (3, format!("📧 [{}] {f}: {}", a.email, s.chars().take(80).collect::<String>()))).collect();
                if new.len() > 3 {
                    lines.push((3, format!("…dan {} email baru lainnya di {}.", new.len() - 3, a.email)));
                }
                crate::notify::toast("NOVA - email baru", &lines[0].1);
                crate::adegan::play(tx, lines);
            }
        }
        let (now, end) = (Utc::now(), Utc::now() + chrono::Duration::hours(24));
        let f = |t: DateTime<Utc>| urlencoding::encode(&t.format("%Y-%m-%dT%H:%M:%SZ").to_string()).into_owned();
        let url = format!("https://graph.microsoft.com/v1.0/me/calendarView?startDateTime={}&endDateTime={}&$top=15&$select=id,subject,start", f(now), f(end));
        if let Some(v) = get_json(http, &tok, url, true).await {
            let mut fresh = vec![];
            for (id, ev) in parse_events(&v) {
                if !seen(&c, &format!("oe{}:{id}", a.email)) && !first {
                    fresh.push(format!("📅 [{}] Acara baru: {} ({})", a.email, ev.title, ev.start.format("%d/%m %H:%M")));
                }
                all_events.push(ev);
            }
            if !fresh.is_empty() {
                crate::adegan::play(tx, fresh.into_iter().take(3).map(|t| (3, t)).collect());
            }
        }
    }
    crate::konteks::set_outlook_events(all_events);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_graph_payloads() {
        let m = serde_json::json!({"value":[{"id":"AAA","subject":"Rapat tim","from":{"emailAddress":{"name":"Budi","address":"b@x.com"}}},{"id":"BBB","subject":"Info","from":{"emailAddress":{"address":"noreply@x.com"}}}]});
        let p = parse_messages(&m);
        assert_eq!(p[0], ("AAA".to_string(), "Budi".to_string(), "Rapat tim".to_string()));
        assert_eq!(p[1].1, "noreply@x.com");
        let e = serde_json::json!({"value":[{"id":"E1","subject":"Review","start":{"dateTime":"2026-10-11T08:00:00.0000000","timeZone":"UTC"}}]});
        let ev = parse_events(&e);
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].1.title, "Review");
        assert_eq!(ev[0].1.start.with_timezone(&Utc).format("%H:%M").to_string(), "08:00");
    }
    #[test]
    fn cfg_defaults_tenant() {
        let c: Cfg = serde_json::from_str("{\"client_id\":\"abc\"}").unwrap();
        assert_eq!(c.tenant, "common");
    }
}
