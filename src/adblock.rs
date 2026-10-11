//! Pemblokir iklan: server DNS lokal. Daftar blokir ada di SQLite (disk), bukan di RAM.
//! Atur DNS sistem ke 127.0.0.1 agar aktif. Port 53 butuh hak admin; jika gagal dipakai port 5353.
use crate::config;
use rusqlite::Connection;
use std::io::{BufRead, BufReader};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;

pub static ON: AtomicBool = AtomicBool::new(false);
pub static BLOCKED: AtomicU64 = AtomicU64::new(0);
pub static TOTAL: AtomicU64 = AtomicU64::new(0);
pub static LISTED: AtomicU64 = AtomicU64::new(0);
static RECENT: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Laporan dari ekstensi browser: iklan di dalam halaman yang diblokir.
pub fn report_external(host: &str) {
    BLOCKED.fetch_add(1, Ordering::Relaxed);
    if let Ok(mut r) = RECENT.lock() {
        if r.len() < 20 {
            r.push(host.to_string());
        }
    }
}

/// Domain iklan terakhir yang diblokir (untuk animasi menembak); mengosongkan antrean.
pub fn take_recent() -> Option<String> {
    let mut r = RECENT.lock().ok()?;
    let d = r.pop();
    r.clear();
    d
}

/// Tugas latar: tiap iklan terblokir -> peliharaan menembaknya (animasi), dengan jeda agar tidak membanjiri.
pub async fn announce(tx: crate::engine::Tx) {
    let mut last_chat = Instant::now() - Duration::from_secs(120);
    loop {
        tokio::time::sleep(Duration::from_millis(800)).await;
        if let Some(d) = take_recent() {
            tx.send(crate::engine::Event::Act { kind: "tembak".into(), text: d.clone(), ms: 2400 });
            if last_chat.elapsed() > Duration::from_secs(45) {
                last_chat = Instant::now();
                tx.sys(format!("🔫 Iklan {d} tertembak! Total diblokir: {}", BLOCKED.load(Ordering::Relaxed)));
            }
            tokio::time::sleep(Duration::from_millis(2400)).await;
        }
    }
}

pub const LIST_URL: &str = "https://raw.githubusercontent.com/StevenBlack/hosts/master/hosts";
const UPSTREAM: &str = "1.1.1.1:53";

const SEED: &[&str] = &[
    "doubleclick.net", "googlesyndication.com", "googleadservices.com", "adservice.google.com", "pagead2.googlesyndication.com",
    "taboola.com", "outbrain.com", "adnxs.com", "criteo.com", "criteo.net", "pubmatic.com", "rubiconproject.com", "openx.net",
    "scorecardresearch.com", "moatads.com", "popads.net", "propellerads.com", "adsterra.com", "mgid.com", "revcontent.com",
    "amazon-adsystem.com", "adsrvr.org", "casalemedia.com", "smartadserver.com", "advertising.com", "yieldmo.com", "ads.yahoo.com",
    "analytics.google.com", "stats.g.doubleclick.net", "hotjar.com", "mc.yandex.ru", "bidswitch.net", "exoclick.com", "juicyads.com",
];

fn db_path() -> std::path::PathBuf {
    config::data_dir().join("iklan.db")
}

fn open() -> rusqlite::Result<Connection> {
    let c = Connection::open(db_path())?;
    c.execute_batch("CREATE TABLE IF NOT EXISTS block(d TEXT PRIMARY KEY) WITHOUT ROWID;")?;
    Ok(c)
}

fn refresh_count(c: &Connection) {
    let n: i64 = c.query_row("SELECT COUNT(*) FROM block", [], |r| r.get(0)).unwrap_or(0);
    LISTED.store(n as u64, Ordering::Relaxed);
}

/// Domain dari satu baris format hosts ("0.0.0.0 x.com") atau daftar domain polos.
fn parse_line(line: &str) -> Option<String> {
    let l = line.split('#').next()?.trim();
    let mut it = l.split_whitespace();
    let a = it.next()?;
    let d = if a == "0.0.0.0" || a == "127.0.0.1" { it.next()? } else { a };
    (d.contains('.') && d != "0.0.0.0" && !d.contains('/')).then(|| d.to_ascii_lowercase())
}

/// Impor streaming dari file (baris demi baris, RAM tetap kecil).
pub fn import_file(path: &std::path::Path) -> anyhow::Result<usize> {
    let mut c = open()?;
    let tx = c.transaction()?;
    let mut n = 0;
    {
        let mut st = tx.prepare("INSERT OR IGNORE INTO block VALUES(?1)")?;
        for line in BufReader::new(std::fs::File::open(path)?).lines().map_while(Result::ok) {
            if let Some(d) = parse_line(&line) {
                st.execute([d])?;
                n += 1;
            }
        }
    }
    tx.commit()?;
    refresh_count(&c);
    Ok(n)
}

fn seed_if_empty() {
    if let Ok(mut c) = open() {
        let n: i64 = c.query_row("SELECT COUNT(*) FROM block", [], |r| r.get(0)).unwrap_or(0);
        if n == 0 {
            if let Ok(tx) = c.transaction() {
                for d in SEED {
                    let _ = tx.execute("INSERT OR IGNORE INTO block VALUES(?1)", [d]);
                }
                let _ = tx.commit();
            }
        }
        refresh_count(&c);
    }
}

/// Unduh daftar ke disk lalu impor. Mengembalikan jumlah domain.
pub async fn update(http: &reqwest::Client) -> anyhow::Result<usize> {
    let bytes = http.get(LIST_URL).send().await?.error_for_status()?.bytes().await?;
    let p = config::data_dir().join("daftar-iklan.txt");
    std::fs::write(&p, &bytes)?;
    drop(bytes);
    tokio::task::spawn_blocking(move || import_file(&p)).await?
}

/// Cocokkan domain dan semua induknya (a.b.c.com -> b.c.com -> c.com).
fn is_blocked(c: &Connection, name: &str) -> bool {
    let mut cur = name;
    while cur.matches('.').count() >= 1 {
        if c.query_row("SELECT 1 FROM block WHERE d=?1", [cur], |_| Ok(())).is_ok() {
            return true;
        }
        match cur.split_once('.') {
            Some((_, rest)) => cur = rest,
            None => break,
        }
    }
    false
}

/// (nama domain huruf kecil, qtype, akhir bagian pertanyaan)
fn parse_query(q: &[u8]) -> Option<(String, u16, usize)> {
    if q.len() < 17 || (q[2] & 0x80) != 0 {
        return None;
    }
    let (mut i, mut name) = (12, String::new());
    loop {
        let l = *q.get(i)? as usize;
        if l == 0 {
            i += 1;
            break;
        }
        if l & 0xC0 != 0 {
            return None;
        }
        if !name.is_empty() {
            name.push('.');
        }
        name.push_str(&String::from_utf8_lossy(q.get(i + 1..i + 1 + l)?).to_ascii_lowercase());
        i += 1 + l;
    }
    let qt = u16::from_be_bytes([*q.get(i)?, *q.get(i + 1)?]);
    Some((name, qt, i + 4))
}

fn blocked_reply(q: &[u8], end: usize, qtype: u16) -> Vec<u8> {
    let mut r = q[..end].to_vec();
    r[2] = 0x81;
    r[3] = 0x80;
    r[6] = 0;
    r[7] = if qtype == 1 { 1 } else { 0 };
    r[8..12].fill(0);
    if qtype == 1 {
        r.extend_from_slice(&[0xC0, 0x0C, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4, 0, 0, 0, 0]);
    }
    r
}

async fn handle(sock: Arc<UdpSocket>, db: Arc<Mutex<Connection>>, q: Vec<u8>, from: SocketAddr) {
    TOTAL.fetch_add(1, Ordering::Relaxed);
    if let Some((name, qt, end)) = parse_query(&q) {
        let blocked = (qt == 1 || qt == 28) && is_blocked(&db.lock().unwrap_or_else(|e| e.into_inner()), &name);
        if blocked {
            BLOCKED.fetch_add(1, Ordering::Relaxed);
            if let Ok(mut r) = RECENT.lock() {
                if r.len() < 20 {
                    r.push(name.clone());
                }
            }
            let _ = sock.send_to(&blocked_reply(&q, end, qt), from).await;
            return;
        }
    }
    // Teruskan ke DNS upstream.
    let Ok(up) = UdpSocket::bind("0.0.0.0:0").await else { return };
    if up.send_to(&q, UPSTREAM).await.is_err() {
        return;
    }
    let mut buf = [0u8; 1500];
    if let Ok(Ok(n)) = tokio::time::timeout(Duration::from_secs(3), up.recv(&mut buf)).await {
        let _ = sock.send_to(&buf[..n], from).await;
    }
}

/// Jalankan server DNS; kembalikan alamat yang dipakai.
pub async fn serve() -> anyhow::Result<(SocketAddr, tokio::task::JoinHandle<()>)> {
    seed_if_empty();
    let sock = match UdpSocket::bind("127.0.0.1:53").await {
        Ok(s) => s,
        Err(_) => UdpSocket::bind("127.0.0.1:5353").await?,
    };
    let addr = sock.local_addr()?;
    let sock = Arc::new(sock);
    let db = Arc::new(Mutex::new(open()?));
    ON.store(true, Ordering::Relaxed);
    let h = tokio::spawn(async move {
        let mut buf = [0u8; 1500];
        loop {
            let Ok((n, from)) = sock.recv_from(&mut buf).await else { continue };
            tokio::spawn(handle(sock.clone(), db.clone(), buf[..n].to_vec(), from));
        }
    });
    Ok((addr, h))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hosts_lines() {
        assert_eq!(parse_line("0.0.0.0 Ads.Example.com # x"), Some("ads.example.com".into()));
        assert_eq!(parse_line("# komentar"), None);
        assert_eq!(parse_line("0.0.0.0 0.0.0.0"), None);
    }
    #[test]
    fn query_roundtrip() {
        let mut q = vec![0x12, 0x34, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0];
        q.extend_from_slice(b"\x03ads\x07example\x03com\x00\x00\x01\x00\x01");
        let (n, t, end) = parse_query(&q).unwrap();
        assert_eq!((n.as_str(), t, end), ("ads.example.com", 1, q.len()));
        let r = blocked_reply(&q, end, 1);
        assert_eq!(&r[2..4], &[0x81, 0x80]);
        assert_eq!(r[7], 1);
        assert_eq!(&r[r.len() - 4..], &[0, 0, 0, 0]);
    }
}
