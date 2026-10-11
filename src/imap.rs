//! Email non-Gmail lewat IMAP (TLS port 993) dengan SANDI APLIKASI: Yahoo, iCloud, domain sendiri, dst. Hanya membaca pengirim +
//! subjek email BELUM DIBACA (BODY.PEEK, tidak menandai terbaca, tidak mengunduh isi). Sandi dienkripsi DPAPI (secure.rs).
//! Catatan: Outlook.com/Microsoft 365 dan Gmail memblokir sandi biasa; Gmail dipakai lewat /google (OAuth).
use crate::config::data_dir;
use crate::engine::Tx;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio_rustls::{rustls, TlsConnector};

#[derive(Serialize, Deserialize, Clone)]
pub struct Acct {
    pub label: String,
    pub host: String,
    pub user: String,
    pub pass_enc: String,
}

fn file() -> std::path::PathBuf {
    data_dir().join("imap_accounts.json")
}

pub fn accounts() -> Vec<Acct> {
    std::fs::read_to_string(file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save(v: &[Acct]) {
    let _ = std::fs::write(file(), serde_json::to_string_pretty(v).unwrap_or_default());
}

fn guess_host(user: &str) -> Option<&'static str> {
    let d = user.rsplit('@').next()?.to_lowercase();
    Some(match d.as_str() {
        "yahoo.com" | "yahoo.co.id" | "ymail.com" => "imap.mail.yahoo.com",
        "icloud.com" | "me.com" => "imap.mail.me.com",
        "zoho.com" => "imap.zoho.com",
        "gmx.com" | "gmx.net" => "imap.gmx.com",
        "aol.com" => "imap.aol.com",
        _ => return None,
    })
}

/// `/email imap <host|auto> <user> <sandi-aplikasi>`
pub fn add(host: &str, user: &str, pass: &str) -> String {
    let l = user.to_lowercase();
    if l.ends_with("@gmail.com") || l.ends_with("@googlemail.com") {
        return "Gmail dipakai lewat /google (OAuth, lebih aman; sandi biasa ditolak Google).".into();
    }
    if ["outlook.com", "hotmail.com", "live.com", "office365.com"].iter().any(|d| l.ends_with(d)) {
        return "Outlook/Hotmail/Microsoft 365 dipakai lewat /outlook (OAuth Microsoft); sandi aplikasi lewat IMAP sudah tidak diterima.".into();
    }
    let host = if host == "auto" { guess_host(user).map(String::from).unwrap_or_default() } else { host.to_string() };
    if host.is_empty() || !user.contains('@') || pass.is_empty() {
        return "Pakai: /email imap <host|auto> <alamat-email> <sandi-aplikasi>. Contoh: /email imap auto saya@yahoo.com abcd-efgh-ijkl".into();
    }
    let Some(enc) = crate::secure::protect(pass) else { return "Gagal mengenkripsi sandi (DPAPI/PowerShell tidak tersedia).".into() };
    let mut v = accounts();
    v.retain(|a| a.label != user);
    v.push(Acct { label: user.to_string(), host: host.clone(), user: user.to_string(), pass_enc: enc });
    save(&v);
    format!("Akun IMAP '{user}' ditambahkan ({host}). Sandi disimpan terenkripsi (DPAPI). Email baru akan kuberitahukan; cek sekarang: 'cek email'.")
}

pub fn remove(label: &str) -> String {
    let mut v = accounts();
    let n = v.len();
    v.retain(|a| a.label != label);
    save(&v);
    if v.len() < n { format!("Akun '{label}' dihapus.") } else { format!("Akun '{label}' tidak ditemukan.") }
}

// ---------- Protokol ----------

fn imap_quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Dekode "encoded-word" RFC 2047 (=?UTF-8?B?...?= / =?UTF-8?Q?...?=).
pub fn decode_words(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find("=?") {
        out.push_str(&rest[..i]);
        let tail = &rest[i + 2..];
        // charset ? enc ? text ?=
        let parsed = (|| {
            let c1 = tail.find('?')?;
            let after = &tail[c1 + 1..];
            let enc = after.get(..1)?;
            if after.get(1..2)? != "?" {
                return None;
            }
            let body = &after[2..];
            let end = body.find("?=")?;
            Some((enc.to_ascii_uppercase(), body[..end].to_string(), c1 + 1 + 2 + end + 2))
        })();
        match parsed {
            Some((enc, text, used)) => {
                let bytes = if enc == "B" {
                    base64::engine::general_purpose::STANDARD.decode(&text).ok()
                } else {
                    let t = text.replace('_', " ");
                    let b = t.as_bytes();
                    let mut v = vec![];
                    let mut k = 0;
                    while k < b.len() {
                        if b[k] == b'=' && k + 2 < b.len() + 0 + 1 && k + 3 <= b.len() {
                            if let Ok(x) = u8::from_str_radix(&t[k + 1..k + 3], 16) {
                                v.push(x);
                                k += 3;
                                continue;
                            }
                        }
                        v.push(b[k]);
                        k += 1;
                    }
                    Some(v)
                };
                out.push_str(&String::from_utf8_lossy(&bytes.unwrap_or_default()));
                rest = &tail[used..];
                // spasi di antara dua encoded-word berurutan diabaikan
                if rest.trim_start().starts_with("=?") {
                    rest = rest.trim_start();
                }
            }
            None => {
                out.push_str("=?");
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Ambil (uid, dari, subjek) dari respons FETCH gabungan.
pub fn parse_fetch(text: &str) -> Vec<(String, String, String)> {
    let mut out = vec![];
    for block in text.split("\n* ").filter(|b| b.contains("FETCH")) {
        let uid = block.split("UID ").nth(1).and_then(|s| s.split(|c: char| !c.is_ascii_digit()).next()).unwrap_or("").to_string();
        let mut from = String::new();
        let mut subject = String::new();
        let mut cur = 0;
        for line in block.lines() {
            let l = line.trim_end();
            if let Some(v) = l.strip_prefix("From:").or_else(|| l.strip_prefix("FROM:")) {
                from = v.trim().to_string();
                cur = 1;
            } else if let Some(v) = l.strip_prefix("Subject:").or_else(|| l.strip_prefix("SUBJECT:")) {
                subject = v.trim().to_string();
                cur = 2;
            } else if (l.starts_with(' ') || l.starts_with('\t')) && cur > 0 {
                if cur == 1 { from.push(' '); from.push_str(l.trim()) } else { subject.push(' '); subject.push_str(l.trim()) }
            } else {
                cur = 0;
            }
        }
        if !uid.is_empty() {
            let from = decode_words(&from);
            let from = from.split('<').next().unwrap_or(&from).trim().trim_matches('"').to_string();
            out.push((uid, from, decode_words(&subject)));
        }
    }
    out
}

async fn read_until_tag<R: tokio::io::AsyncBufRead + Unpin>(r: &mut R, tag: &str) -> Result<String, String> {
    let mut all = String::new();
    loop {
        let mut line = String::new();
        let n = tokio::time::timeout(Duration::from_secs(20), r.read_line(&mut line)).await.map_err(|_| "waktu habis".to_string())?.map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("koneksi terputus".into());
        }
        if line.starts_with(&format!("{tag} ")) {
            return if line[tag.len() + 1..].starts_with("OK") { Ok(all) } else { Err(line.trim().to_string()) };
        }
        all.push_str(&line.replace("\r\n", "\n"));
    }
}

/// Email belum dibaca (maks 8 terbaru): (uid, dari, subjek).
pub async fn unread(a: &Acct) -> Result<Vec<(String, String, String)>, String> {
    let pass = crate::secure::unprotect(&a.pass_enc).ok_or("sandi tidak bisa dibuka")?;
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let cfg = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_root_certificates(roots)
        .with_no_client_auth();
    let tcp = tokio::time::timeout(Duration::from_secs(15), tokio::net::TcpStream::connect((a.host.as_str(), 993))).await.map_err(|_| "tidak bisa terhubung (waktu habis)".to_string())?.map_err(|e| e.to_string())?;
    let name = rustls::pki_types::ServerName::try_from(a.host.clone()).map_err(|e| e.to_string())?;
    let tls = TlsConnector::from(Arc::new(cfg)).connect(name, tcp).await.map_err(|e| e.to_string())?;
    let (rd, mut wr) = tokio::io::split(tls);
    let mut rd = BufReader::new(rd);
    let mut greet = String::new();
    rd.read_line(&mut greet).await.map_err(|e| e.to_string())?;
    let l = format!("a1 LOGIN {} {}\r\n", imap_quote(&a.user), imap_quote(&pass));
    wr.write_all(l.as_bytes()).await.map_err(|e| e.to_string())?;
    read_until_tag(&mut rd, "a1").await.map_err(|e| format!("login gagal ({e}); pakai sandi aplikasi, bukan sandi biasa"))?;
    wr.write_all(b"a2 EXAMINE INBOX\r\n").await.map_err(|e| e.to_string())?; // EXAMINE = baca-saja
    read_until_tag(&mut rd, "a2").await?;
    wr.write_all(b"a3 UID SEARCH UNSEEN\r\n").await.map_err(|e| e.to_string())?;
    let res = read_until_tag(&mut rd, "a3").await?;
    let mut uids: Vec<String> = res.lines().find(|l| l.starts_with("* SEARCH")).map(|l| l.split_whitespace().skip(2).map(String::from).collect()).unwrap_or_default();
    let keep = uids.len().saturating_sub(8);
    uids.drain(..keep);
    let mut out = vec![];
    if !uids.is_empty() {
        let l = format!("a4 UID FETCH {} (BODY.PEEK[HEADER.FIELDS (FROM SUBJECT)])\r\n", uids.join(","));
        wr.write_all(l.as_bytes()).await.map_err(|e| e.to_string())?;
        let body = read_until_tag(&mut rd, "a4").await?;
        out = parse_fetch(&format!("\n{body}"));
    }
    let _ = wr.write_all(b"a5 LOGOUT\r\n").await;
    Ok(out)
}

fn db() -> Option<rusqlite::Connection> {
    let c = crate::assistant::db().ok()?;
    let _ = c.execute_batch("CREATE TABLE IF NOT EXISTS mail_seen(key TEXT PRIMARY KEY); CREATE TABLE IF NOT EXISTS mail_base(label TEXT PRIMARY KEY);");
    Some(c)
}

/// Pantau semua akun IMAP; pesan baru diberitahukan (akun baru: dasar saja, tanpa membanjiri).
pub async fn poll(tx: &Tx) {
    let Some(c) = db() else { return };
    for a in accounts() {
        let first = c.query_row("SELECT 1 FROM mail_base WHERE label=?1", [&a.label], |_| Ok(())).is_err();
        let Ok(list) = unread(&a).await else { continue };
        let mut fresh = vec![];
        for (uid, from, subj) in list {
            let new = c.execute("INSERT OR IGNORE INTO mail_seen VALUES(?1)", [format!("{}:{uid}", a.label)]).map(|n| n == 1).unwrap_or(false);
            if new && !first {
                fresh.push((from, subj));
            }
        }
        if first {
            let _ = c.execute("INSERT OR IGNORE INTO mail_base VALUES(?1)", [&a.label]);
        }
        if !fresh.is_empty() {
            let mut lines: Vec<(usize, String)> = fresh.iter().take(3).map(|(f, s)| (3, format!("📧 [{}] {f}: {}", a.label, s.chars().take(80).collect::<String>()))).collect();
            if fresh.len() > 3 {
                lines.push((3, format!("…dan {} email baru lainnya di {}.", fresh.len() - 3, a.label)));
            }
            crate::notify::toast("NOVA - email baru", &lines[0].1);
            crate::adegan::play(tx, lines);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rfc2047_and_fetch_parsing() {
        assert_eq!(decode_words("=?UTF-8?B?SGFsbyBkdW5pYQ==?="), "Halo dunia");
        assert_eq!(decode_words("=?UTF-8?Q?Laporan_Bulan_=C3=A9?="), "Laporan Bulan é");
        assert_eq!(decode_words("tanpa kode"), "tanpa kode");
        let r = "\n* 3 FETCH (UID 12 BODY[HEADER.FIELDS (FROM SUBJECT)] {80}\nFrom: \"Budi S\" <budi@x.com>\nSubject: =?UTF-8?B?UmFwYXQ=?= besok\n  jam 9\n\n)\n* 4 FETCH (UID 15 BODY[HEADER.FIELDS (FROM SUBJECT)] {40}\nFrom: info@bank.co.id\nSubject: Tagihan\n\n)\n";
        let v = parse_fetch(r);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0], ("12".to_string(), "Budi S".to_string(), "Rapat besok jam 9".to_string()));
        assert_eq!(v[1].1, "info@bank.co.id");
    }
    #[test]
    fn host_guess_and_quote() {
        assert_eq!(guess_host("a@yahoo.com"), Some("imap.mail.yahoo.com"));
        assert_eq!(imap_quote("a\"b"), "\"a\\\"b\"");
        assert!(add("auto", "x@gmail.com", "p").contains("/google"));
    }
}
