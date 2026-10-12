//! "Wireshark mini" NOVA, dua tingkat:
//! 1. Monitor koneksi (tanpa admin): koneksi TCP per aplikasi, domain (cache DNS), protokol dari nomor port, laju unduh/unggah.
//! 2. Tangkap paket (admin, atas persetujuan UAC): `pktmon` bawaan Windows → pcapng → diurai di sini (DNS, SNI TLS, HTTP,
//!    SYN gagal, RST, retransmisi, penutur teratas). Hanya header dan awal paket (512 byte) yang ditangkap; berkas dihapus setelah diurai.
use serde::Serialize;
use std::collections::HashMap;

#[derive(Serialize, Clone, Default)]
pub struct Koneksi {
    pub proses: String,
    pub remote: String,
    pub port: u32,
    pub protokol: String,
    pub status: String,
    pub domain: String,
}

#[derive(Serialize, Clone, Default)]
pub struct Monitor {
    pub waktu: String,
    pub unduh_kbps: u64,
    pub unggah_kbps: u64,
    pub koneksi: Vec<Koneksi>,
    pub per_proses: Vec<(String, usize)>,
    pub catatan: Vec<String>,
}

pub fn protokol(port: u32) -> &'static str {
    match port {
        80 | 8080 => "HTTP",
        443 | 8443 => "HTTPS/TLS",
        53 => "DNS",
        22 => "SSH",
        21 => "FTP",
        25 | 465 | 587 => "SMTP",
        110 | 995 => "POP3",
        143 | 993 => "IMAP",
        3389 => "RDP",
        445 | 139 => "SMB",
        5228 => "Google push",
        1883 | 8883 => "MQTT",
        _ => "lain",
    }
}

fn ps(script: &str) -> String {
    let mut c = std::process::Command::new("powershell");
    c.args(["-NoProfile", "-Command", script]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    c.output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default()
}

const PS_KONEKSI: &str = r#"$d=@{}; Get-DnsClientCache -ErrorAction SilentlyContinue | Where-Object { $_.Type -in 1,28 } | ForEach-Object { $d[$_.Data]=$_.Entry }
$s1=Get-NetAdapterStatistics -ErrorAction SilentlyContinue; Start-Sleep -Milliseconds 800; $s2=Get-NetAdapterStatistics -ErrorAction SilentlyContinue
'RATE|' + (($s2|Measure-Object ReceivedBytes -Sum).Sum-($s1|Measure-Object ReceivedBytes -Sum).Sum) + '|' + (($s2|Measure-Object SentBytes -Sum).Sum-($s1|Measure-Object SentBytes -Sum).Sum)
Get-NetTCPConnection -State Established -ErrorAction SilentlyContinue | Where-Object { $_.RemoteAddress -notin '127.0.0.1','::1' } | ForEach-Object { $p=(Get-Process -Id $_.OwningProcess -ErrorAction SilentlyContinue).ProcessName; "C|$p|$($_.RemoteAddress)|$($_.RemotePort)|$($_.State)|$($d[$_.RemoteAddress])" }"#;

pub fn urai_monitor(out: &str) -> Monitor {
    let mut m = Monitor { waktu: chrono::Local::now().format("%H:%M:%S").to_string(), ..Default::default() };
    for l in out.lines() {
        let p: Vec<&str> = l.trim().split('|').collect();
        match p.first() {
            Some(&"RATE") if p.len() >= 3 => {
                // diukur dalam ~0,8 detik → kilobit/dtk
                let kbps = |s: &str| s.parse::<i64>().unwrap_or(0).max(0) as u64 * 8 * 10 / 8 / 1000;
                m.unduh_kbps = kbps(p[1]);
                m.unggah_kbps = kbps(p[2]);
            }
            Some(&"C") if p.len() >= 6 => {
                let port = p[3].parse().unwrap_or(0);
                m.koneksi.push(Koneksi { proses: p[1].to_string(), remote: p[2].to_string(), port, protokol: protokol(port).to_string(), status: p[4].to_string(), domain: p[5].to_string() });
            }
            _ => {}
        }
    }
    let mut per: HashMap<String, usize> = HashMap::new();
    for k in &m.koneksi {
        *per.entry(k.proses.clone()).or_insert(0) += 1;
    }
    let mut v: Vec<_> = per.into_iter().filter(|(k, _)| !k.is_empty()).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    m.per_proses = v.into_iter().take(10).collect();
    for (nama, n) in &m.per_proses {
        if *n >= 40 {
            m.catatan.push(format!("{nama} membuka {n} koneksi sekaligus (tidak biasa; cek bila bukan peramban/unduhan)."));
        }
    }
    for k in &m.koneksi {
        if k.protokol == "lain" && !(1024..=65535).contains(&k.port) {
            m.catatan.push(format!("{} → {}:{} (port tidak lazim)", k.proses, k.remote, k.port));
        } else if matches!(k.protokol.as_str(), "RDP" | "SSH" | "FTP" | "SMB") {
            m.catatan.push(format!("{} memakai {} ke {}:{}", k.proses, k.protokol, k.remote, k.port));
        }
    }
    m.catatan.truncate(8);
    m
}

pub async fn monitor() -> Monitor {
    tokio::task::spawn_blocking(|| urai_monitor(&ps(PS_KONEKSI))).await.unwrap_or_default()
}

pub fn teks_monitor(m: &Monitor) -> String {
    let mut s = format!("📡 Monitor koneksi {} — unduh {} kb/s · unggah {} kb/s · {} koneksi aktif\n", m.waktu, m.unduh_kbps, m.unggah_kbps, m.koneksi.len());
    s.push_str(&format!("Per aplikasi: {}\n", m.per_proses.iter().map(|(k, n)| format!("{k} ({n})")).collect::<Vec<_>>().join(", ")));
    let mut domain: HashMap<&str, usize> = HashMap::new();
    for k in m.koneksi.iter().filter(|k| !k.domain.is_empty()) {
        *domain.entry(k.domain.as_str()).or_insert(0) += 1;
    }
    let mut d: Vec<_> = domain.into_iter().collect();
    d.sort_by(|a, b| b.1.cmp(&a.1));
    if !d.is_empty() {
        s.push_str(&format!("Domain: {}\n", d.iter().take(8).map(|(k, n)| format!("{k} ({n})")).collect::<Vec<_>>().join(", ")));
    }
    if m.catatan.is_empty() {
        s.push_str("Tidak ada yang mencurigakan dari pola koneksi.");
    } else {
        s.push_str(&format!("Perlu dilihat: {}", m.catatan.join(" · ")));
    }
    s
}

// ---------- Tingkat 2: penangkap paket ----------

#[derive(Serialize, Clone, Default)]
pub struct Analisis {
    pub paket: usize,
    pub byte: u64,
    pub protokol: Vec<(String, usize)>,
    pub penutur: Vec<(String, u64)>,
    pub dns: Vec<(String, usize)>,
    pub sni: Vec<(String, usize)>,
    pub http: Vec<String>,
    pub syn_gagal: usize,
    pub rst: usize,
    pub retransmisi: usize,
}

fn u16be(b: &[u8], i: usize) -> usize {
    b.get(i..i + 2).map(|x| u16::from_be_bytes([x[0], x[1]]) as usize).unwrap_or(0)
}

/// Nama dari bagian pertanyaan DNS (tanpa kompresi, cukup untuk query).
fn dns_nama(d: &[u8]) -> Option<String> {
    let mut i = 12;
    let mut parts = vec![];
    loop {
        let n = *d.get(i)? as usize;
        if n == 0 {
            break;
        }
        if n & 0xC0 != 0 {
            return None;
        }
        parts.push(String::from_utf8_lossy(d.get(i + 1..i + 1 + n)?).to_string());
        i += n + 1;
    }
    (!parts.is_empty()).then(|| parts.join("."))
}

/// SNI dari TLS ClientHello.
pub fn sni(p: &[u8]) -> Option<String> {
    if p.len() < 43 || p[0] != 0x16 || p[5] != 0x01 {
        return None;
    }
    let mut i = 43; // rekaman(5) + handshake(4) + versi(2) + acak(32)
    i += 1 + *p.get(i)? as usize; // session id
    i += 2 + u16be(p, i); // cipher suites
    i += 1 + *p.get(i)? as usize; // kompresi
    let end = i + 2 + u16be(p, i);
    i += 2;
    while i + 4 <= end.min(p.len()) {
        let (t, l) = (u16be(p, i), u16be(p, i + 2));
        if t == 0 {
            // server_name: daftar(2) tipe(1) panjang(2) nama
            let n = u16be(p, i + 7);
            return p.get(i + 9..i + 9 + n).map(|b| String::from_utf8_lossy(b).to_string());
        }
        i += 4 + l;
    }
    None
}

fn ip_str(b: &[u8]) -> String {
    if b.len() == 4 {
        format!("{}.{}.{}.{}", b[0], b[1], b[2], b[3])
    } else {
        b.chunks(2).map(|c| format!("{:x}", u16::from_be_bytes([c[0], c.get(1).copied().unwrap_or(0)]))).collect::<Vec<_>>().join(":")
    }
}

/// Urai satu bingkai Ethernet ke dalam `a`.
fn bingkai(f: &[u8], a: &mut Analisis, st: &mut Status) {
    a.paket += 1;
    a.byte += f.len() as u64;
    let mut et = u16be(f, 12);
    let mut off = 14;
    if et == 0x8100 {
        et = u16be(f, 16);
        off = 18;
    }
    let (proto, src, dst, l4) = match et {
        0x0800 if f.len() >= off + 20 => {
            let ihl = (f[off] & 0x0F) as usize * 4;
            (f[off + 9], ip_str(&f[off + 12..off + 16]), ip_str(&f[off + 16..off + 20]), off + ihl)
        }
        0x86DD if f.len() >= off + 40 => (f[off + 6], ip_str(&f[off + 8..off + 24]), ip_str(&f[off + 24..off + 40]), off + 40),
        _ => {
            *st.proto.entry("lain".into()).or_insert(0) += 1;
            return;
        }
    };
    *st.talker.entry(if src.starts_with("192.168.") || src.starts_with("10.") || src.starts_with("172.") { dst.clone() } else { src.clone() }).or_insert(0) += f.len() as u64;
    let nama = match proto {
        6 if f.len() >= l4 + 20 => {
            let (sp, dp) = (u16be(f, l4), u16be(f, l4 + 2));
            let doff = (f[l4 + 12] >> 4) as usize * 4;
            let fl = f[l4 + 13];
            let seq = u32::from_be_bytes([f[l4 + 4], f[l4 + 5], f[l4 + 6], f[l4 + 7]]);
            let data = f.get(l4 + doff..).unwrap_or(&[]);
            if fl & 0x02 != 0 && fl & 0x10 == 0 {
                st.syn.insert((src.clone(), sp, dst.clone(), dp));
            }
            if fl & 0x12 == 0x12 {
                st.synack.insert((dst.clone(), dp, src.clone(), sp));
            }
            if fl & 0x04 != 0 {
                a.rst += 1;
            }
            if !data.is_empty() && !st.seen.insert((src.clone(), sp, dst.clone(), dp, seq)) {
                a.retransmisi += 1;
            }
            if let Some(h) = sni(data) {
                *st.sni.entry(h.clone()).or_insert(0) += 1;
                st.nama_ip.insert(dst.clone(), h);
                "TLS"
            } else if [b"GET ".as_slice(), b"POST", b"HEAD", b"PUT "].iter().any(|m| data.starts_with(m)) {
                if let Some(l) = String::from_utf8_lossy(&data[..data.len().min(200)]).lines().next() {
                    if a.http.len() < 10 {
                        a.http.push(format!("{src} → {dst}: {l}"));
                    }
                }
                "HTTP"
            } else if dp == 443 || sp == 443 {
                "TLS"
            } else {
                "TCP"
            }
        }
        17 if f.len() >= l4 + 8 => {
            let (sp, dp) = (u16be(f, l4), u16be(f, l4 + 2));
            if dp == 53 || sp == 53 {
                if let Some(n) = dns_nama(f.get(l4 + 8..).unwrap_or(&[])) {
                    *st.dns.entry(n).or_insert(0) += 1;
                }
                "DNS"
            } else if dp == 443 || sp == 443 {
                "QUIC"
            } else {
                "UDP"
            }
        }
        1 | 58 => "ICMP",
        _ => "lain",
    };
    *st.proto.entry(nama.into()).or_insert(0) += 1;
}

#[derive(Default)]
struct Status {
    proto: HashMap<String, usize>,
    talker: HashMap<String, u64>,
    dns: HashMap<String, usize>,
    sni: HashMap<String, usize>,
    nama_ip: HashMap<String, String>,
    syn: std::collections::HashSet<(String, usize, String, usize)>,
    synack: std::collections::HashSet<(String, usize, String, usize)>,
    seen: std::collections::HashSet<(String, usize, String, usize, u32)>,
}

fn urut<K: Clone, V: Ord + Copy>(m: &HashMap<K, V>, n: usize) -> Vec<(K, V)> {
    let mut v: Vec<(K, V)> = m.iter().map(|(k, v)| (k.clone(), *v)).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    v.truncate(n);
    v
}

/// Urai berkas pcapng/pcap (Ethernet saja) menjadi ringkasan.
pub fn urai_pcap(b: &[u8]) -> Analisis {
    let (mut a, mut st) = (Analisis::default(), Status::default());
    let le32 = |i: usize| b.get(i..i + 4).map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]) as usize).unwrap_or(0);
    if b.len() >= 4 && le32(0) == 0xA1B2C3D4 {
        // pcap klasik
        let mut i = 24;
        while i + 16 <= b.len() {
            let cap = le32(i + 8);
            if let Some(f) = b.get(i + 16..i + 16 + cap) {
                bingkai(f, &mut a, &mut st);
            }
            i += 16 + cap;
        }
    } else {
        let mut i = 0;
        while i + 12 <= b.len() {
            let (t, len) = (le32(i), le32(i + 4));
            if len < 12 {
                break;
            }
            if t == 6 {
                let cap = le32(i + 20);
                if let Some(f) = b.get(i + 28..i + 28 + cap) {
                    bingkai(f, &mut a, &mut st);
                }
            }
            i += len;
        }
    }
    a.syn_gagal = st.syn.iter().filter(|k| !st.synack.contains(*k)).count();
    a.protokol = urut(&st.proto, 8);
    a.penutur = urut(&st.talker, 8).into_iter().map(|(ip, n)| (st.nama_ip.get(&ip).map(|h| format!("{h} ({ip})")).unwrap_or(ip), n)).collect();
    a.dns = urut(&st.dns, 10);
    a.sni = urut(&st.sni, 10);
    a
}

pub fn teks_analisis(a: &Analisis, detik: u32) -> String {
    let mut s = format!("🦈 Tangkap paket {detik} dtk: {} paket, {:.1} KB\n", a.paket, a.byte as f64 / 1024.0);
    s.push_str(&format!("Protokol: {}\n", a.protokol.iter().map(|(k, n)| format!("{k} {n}")).collect::<Vec<_>>().join(", ")));
    s.push_str(&format!("Penutur teratas: {}\n", a.penutur.iter().take(6).map(|(k, n)| format!("{k} {:.0} KB", *n as f64 / 1024.0)).collect::<Vec<_>>().join(", ")));
    if !a.dns.is_empty() {
        s.push_str(&format!("DNS: {}\n", a.dns.iter().map(|(k, n)| format!("{k} ({n})")).collect::<Vec<_>>().join(", ")));
    }
    if !a.sni.is_empty() {
        s.push_str(&format!("TLS SNI: {}\n", a.sni.iter().map(|(k, n)| format!("{k} ({n})")).collect::<Vec<_>>().join(", ")));
    }
    for h in &a.http {
        s.push_str(&format!("HTTP (tanpa enkripsi): {h}\n"));
    }
    s.push_str(&format!("Kesehatan: SYN tanpa balasan {} · RST {} · retransmisi {}", a.syn_gagal, a.rst, a.retransmisi));
    if a.retransmisi * 20 > a.paket.max(1) {
        s.push_str(" — retransmisi tinggi: jaringan tidak stabil/paket hilang.");
    }
    if a.syn_gagal >= 5 {
        s.push_str(" — banyak sambungan gagal: server tidak menjawab atau diblokir firewall.");
    }
    s
}

fn tmp(n: &str) -> std::path::PathBuf {
    crate::config::data_dir().join(n)
}

/// Tangkap `detik` detik lewat pktmon (UAC muncul sekali), urai, lalu hapus berkas tangkapan.
pub async fn tangkap(detik: u32) -> String {
    let detik = detik.clamp(5, 60);
    let (etl, pcap, skrip) = (tmp("cap.etl"), tmp("cap.pcapng"), tmp("cap.ps1"));
    let _ = std::fs::remove_file(&pcap);
    let s = format!(
        "pktmon stop | Out-Null\npktmon filter remove | Out-Null\npktmon start --capture --comp nics --pkt-size 512 -f \"{etl}\" | Out-Null\nStart-Sleep -Seconds {detik}\npktmon stop | Out-Null\npktmon etl2pcap \"{etl}\" --out \"{pcap}\" | Out-Null\n",
        etl = etl.display(),
        pcap = pcap.display()
    );
    if std::fs::write(&skrip, s).is_err() {
        return "Gagal menulis skrip tangkapan.".into();
    }
    let cmd = format!("Start-Process powershell -Verb RunAs -Wait -WindowStyle Hidden -ArgumentList '-NoProfile','-ExecutionPolicy','Bypass','-File','{}'", skrip.display());
    let _ = tokio::task::spawn_blocking(move || ps(&cmd)).await;
    let r = match std::fs::read(&pcap) {
        Ok(b) if b.len() > 32 => teks_analisis(&urai_pcap(&b), detik),
        _ => "Tangkapan tidak dihasilkan (UAC ditolak, atau pktmon tidak tersedia di Windows ini).".into(),
    };
    for f in [&etl, &pcap, &skrip] {
        let _ = std::fs::remove_file(f);
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eth_ip_udp_dns(nama: &str) -> Vec<u8> {
        let mut q = vec![0x12, 0x34, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0];
        for p in nama.split('.') {
            q.push(p.len() as u8);
            q.extend(p.bytes());
        }
        q.extend([0, 0, 1, 0, 1]);
        let mut f = vec![0u8; 12];
        f.extend([0x08, 0x00]);
        let total = 20 + 8 + q.len();
        f.extend([0x45, 0, (total >> 8) as u8, total as u8, 0, 0, 0, 0, 64, 17, 0, 0, 192, 168, 1, 5, 8, 8, 8, 8]);
        f.extend([0xC0, 0x01, 0, 53, 0, (8 + q.len()) as u8, 0, 0]);
        f.extend(q);
        f
    }

    fn pcapng(frames: &[Vec<u8>]) -> Vec<u8> {
        let mut b = vec![];
        b.extend(0x0A0D0D0Au32.to_le_bytes());
        b.extend(28u32.to_le_bytes());
        b.extend([0x4D, 0x3C, 0x2B, 0x1A, 1, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        b.extend(28u32.to_le_bytes());
        for f in frames {
            let pad = (4 - f.len() % 4) % 4;
            let len = 32 + f.len() + pad;
            b.extend(6u32.to_le_bytes());
            b.extend((len as u32).to_le_bytes());
            b.extend([0u8; 12]);
            b.extend((f.len() as u32).to_le_bytes());
            b.extend((f.len() as u32).to_le_bytes());
            b.extend(f);
            b.extend(vec![0u8; pad]);
            b.extend((len as u32).to_le_bytes());
        }
        b
    }

    #[test]
    fn parses_dns_from_pcapng() {
        let a = urai_pcap(&pcapng(&[eth_ip_udp_dns("example.com"), eth_ip_udp_dns("example.com"), eth_ip_udp_dns("rust-lang.org")]));
        assert_eq!(a.paket, 3);
        assert_eq!(a.dns[0], ("example.com".to_string(), 2));
        assert!(a.protokol.iter().any(|(k, n)| k == "DNS" && *n == 3));
    }

    #[test]
    fn extracts_sni() {
        let host = b"youtube.com";
        let mut ext = vec![0, 0];
        let sn = [&[0u8, (host.len() + 3) as u8, 0, (host.len() >> 8) as u8, host.len() as u8][..], host].concat();
        ext.extend([(sn.len() >> 8) as u8, sn.len() as u8]);
        ext.extend(sn);
        let mut hs = vec![3, 3];
        hs.extend([0u8; 32]);
        hs.push(0);
        hs.extend([0, 2, 0x13, 0x01]);
        hs.extend([1, 0]);
        hs.extend([(ext.len() >> 8) as u8, ext.len() as u8]);
        hs.extend(ext);
        let mut p = vec![0x16, 3, 1, ((hs.len() + 4) >> 8) as u8, (hs.len() + 4) as u8, 1, 0, (hs.len() >> 8) as u8, hs.len() as u8];
        p.extend(hs);
        assert_eq!(sni(&p).as_deref(), Some("youtube.com"));
    }

    #[test]
    fn monitor_parsing_and_flags() {
        let m = urai_monitor("RATE|100000|20000\nC|chrome|142.250.1.1|443|Established|www.google.com\nC|evil|1.2.3.4|23|Established|\n");
        assert_eq!(m.koneksi.len(), 2);
        assert_eq!(m.koneksi[0].protokol, "HTTPS/TLS");
        assert_eq!(m.koneksi[0].domain, "www.google.com");
        assert!(m.catatan.iter().any(|c| c.contains("port tidak lazim")));
    }
}
