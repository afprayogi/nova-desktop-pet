//! Diagnosa jalur internet (hanya membaca): Wi-Fi → gateway/router → internet (ping) → DNS → web → tracert, ditambah
//! aplikasi yang paling banyak membuka koneksi. Aturan menyimpulkan bagian mana yang bermasalah dan apa yang kurang.
use serde::Serialize;
use std::sync::Mutex;
use std::time::Instant;

#[derive(Serialize, Clone, Default)]
pub struct Laporan {
    pub waktu: String,
    pub wifi_ssid: String,
    pub sinyal: Option<u32>,
    pub gateway: String,
    pub dns: String,
    pub ping_gateway_ms: Option<u32>,
    pub loss_gateway: Option<u32>,
    pub ping_internet_ms: Option<u32>,
    pub loss_internet: Option<u32>,
    pub dns_ms: Option<u32>,
    pub web_ms: Option<u32>,
    pub hop: Vec<String>,
    pub top_proses: Vec<String>,
    pub kesimpulan: Vec<String>,
}

static TERAKHIR: Mutex<Option<Laporan>> = Mutex::new(None);

pub fn terakhir() -> Option<Laporan> {
    TERAKHIR.lock().ok()?.clone()
}

fn jalankan(prog: &str, args: &[&str]) -> String {
    let mut c = std::process::Command::new(prog);
    c.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    c.output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default()
}

fn ps(script: &str) -> String {
    jalankan("powershell", &["-NoProfile", "-Command", script])
}

/// (rata-rata ms, persen paket hilang) dari keluaran `ping` (bahasa Windows apa pun: ambil angka sebelum "ms" terakhir dan "%").
fn ping(host: &str) -> (Option<u32>, Option<u32>) {
    let o = jalankan("ping", &["-n", "4", "-w", "1500", host]);
    let ms = regex::Regex::new(r"(\d+)\s*ms").unwrap().captures_iter(&o).last().and_then(|c| c[1].parse().ok());
    let loss = regex::Regex::new(r"(\d+)%").unwrap().captures(&o).and_then(|c| c[1].parse().ok());
    // semua paket hilang: tidak ada ms yang bermakna
    (if loss == Some(100) { None } else { ms }, loss)
}

fn kunci(teks: &str, k: &str) -> String {
    teks.lines().find_map(|l| l.strip_prefix(k)).unwrap_or("").trim().to_string()
}

fn wifi() -> (String, Option<u32>) {
    let o = jalankan("netsh", &["wlan", "show", "interfaces"]);
    let ssid = o.lines().filter(|l| l.trim_start().starts_with("SSID") && !l.contains("BSSID")).find_map(|l| l.split_once(':')).map(|x| x.1.trim().to_string()).unwrap_or_default();
    let sinyal = o.lines().find(|l| l.contains("Signal") || l.contains("Sinyal")).and_then(|l| l.split_once(':')).and_then(|x| x.1.trim().trim_end_matches('%').parse().ok());
    (ssid, sinyal)
}

fn tracert() -> Vec<String> {
    let o = jalankan("tracert", &["-d", "-h", "7", "-w", "700", "1.1.1.1"]);
    let t = regex::Regex::new(r"(\d+)\s*ms|<1 ms|\*").unwrap();
    o.lines()
        .filter_map(|l| {
            let l = l.trim();
            let n: u32 = l.split_whitespace().next()?.parse().ok()?;
            let ip = l.split_whitespace().last().filter(|x| x.parse::<std::net::IpAddr>().is_ok()).unwrap_or("?");
            let ms: Vec<String> = t.find_iter(l).map(|m| m.as_str().replace(" ms", "").replace("ms", "")).collect();
            Some(format!("{n}: {ip} [{}]", ms.join(" ")))
        })
        .collect()
}

fn top_proses() -> Vec<String> {
    ps("Get-NetTCPConnection -State Established -ErrorAction SilentlyContinue | Group-Object OwningProcess | Sort-Object Count -Descending | Select-Object -First 5 | ForEach-Object { $p=(Get-Process -Id $_.Name -ErrorAction SilentlyContinue).ProcessName; if($p){ $p + ':' + $_.Count } }")
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// Aturan diagnosa: urut dari yang paling dekat ke laptop sampai ke internet.
pub fn simpulkan(r: &Laporan) -> Vec<String> {
    let mut k = vec![];
    if r.gateway.is_empty() {
        k.push("Tidak ada jalur ke jaringan (gateway tidak ada): Wi-Fi/kabel belum tersambung. Sambungkan ulang ke router.".into());
        return k;
    }
    let gw_buruk = r.loss_gateway.unwrap_or(0) >= 20 || r.ping_gateway_ms.is_some_and(|m| m > 80) || (r.ping_gateway_ms.is_none() && r.loss_gateway.is_some());
    if gw_buruk {
        let mut s = format!("Masalah di jaringan lokal (laptop → router {}): ping {} ms, hilang {}%.", r.gateway, r.ping_gateway_ms.map(|m| m.to_string()).unwrap_or("-".into()), r.loss_gateway.unwrap_or(0));
        if r.sinyal.is_some_and(|s| s < 50) {
            s.push_str(&format!(" Sinyal Wi-Fi lemah ({}%): dekatkan ke router atau pindah ke 5 GHz.", r.sinyal.unwrap_or(0)));
        } else {
            s.push_str(" Coba matikan perangkat lain yang sedang mengunduh, atau restart router.");
        }
        k.push(s);
    }
    if !gw_buruk && (r.ping_internet_ms.is_none() || r.loss_internet == Some(100)) {
        k.push("Router normal tetapi internet tidak terjangkau: masalah di modem/ISP. Restart modem; bila tetap, hubungi ISP.".into());
    } else if r.loss_internet.unwrap_or(0) >= 10 || r.ping_internet_ms.is_some_and(|m| m > 150) {
        k.push(format!("Internet lambat/tidak stabil: ping {} ms, paket hilang {}%. Router sehat, jadi kemungkinan di ISP atau jalur ke luar.", r.ping_internet_ms.map(|m| m.to_string()).unwrap_or("-".into()), r.loss_internet.unwrap_or(0)));
    }
    if r.ping_internet_ms.is_some() && r.dns_ms.is_none() {
        k.push("Ping lolos tetapi DNS gagal: ganti DNS ke 1.1.1.1 atau 8.8.8.8 di pengaturan jaringan.".into());
    } else if r.dns_ms.is_some_and(|m| m > 300) {
        k.push(format!("DNS lambat ({} ms): pertimbangkan 1.1.1.1 atau 8.8.8.8.", r.dns_ms.unwrap_or(0)));
    }
    if r.ping_internet_ms.is_some() && r.dns_ms.is_some() && r.web_ms.is_none() {
        k.push("Ping dan DNS normal tetapi web gagal dibuka: cek VPN/proxy/firewall/antivirus.".into());
    }
    if k.is_empty() {
        k.push(format!("Jalur internet sehat: router {} ms, internet {} ms, DNS {} ms, web {} ms.", r.ping_gateway_ms.unwrap_or(0), r.ping_internet_ms.unwrap_or(0), r.dns_ms.unwrap_or(0), r.web_ms.unwrap_or(0)));
    }
    k
}

pub async fn periksa(http: &reqwest::Client) -> Laporan {
    let mut r = tokio::task::spawn_blocking(|| {
        let (ssid, sinyal) = wifi();
        let info = ps("$r=Get-NetRoute -DestinationPrefix 0.0.0.0/0 -ErrorAction SilentlyContinue | Sort-Object RouteMetric | Select-Object -First 1; 'gateway=' + $r.NextHop; 'dns=' + ((Get-DnsClientServerAddress -AddressFamily IPv4 -ErrorAction SilentlyContinue | Where-Object { $_.ServerAddresses } | Select-Object -First 1).ServerAddresses -join ',')");
        let gateway = kunci(&info, "gateway=");
        let (pg, lg) = if gateway.is_empty() { (None, None) } else { ping(&gateway) };
        let (pi, li) = ping("1.1.1.1");
        let t = Instant::now();
        let dns_ok = std::net::ToSocketAddrs::to_socket_addrs(&("example.com", 80)).is_ok();
        let dns_ms = dns_ok.then(|| t.elapsed().as_millis() as u32);
        Laporan { waktu: chrono::Local::now().format("%H:%M:%S").to_string(), wifi_ssid: ssid, sinyal, gateway, dns: kunci(&info, "dns="), ping_gateway_ms: pg, loss_gateway: lg, ping_internet_ms: pi, loss_internet: li, dns_ms, hop: tracert(), top_proses: top_proses(), ..Default::default() }
    })
    .await
    .unwrap_or_default();
    let t = Instant::now();
    r.web_ms = http.get("http://connectivitycheck.gstatic.com/generate_204").timeout(std::time::Duration::from_secs(6)).send().await.ok().map(|_| t.elapsed().as_millis() as u32);
    r.kesimpulan = simpulkan(&r);
    if let Ok(mut g) = TERAKHIR.lock() {
        *g = Some(r.clone());
    }
    r
}

pub fn teks(r: &Laporan) -> String {
    let mut s = format!(
        "🌐 Jalur internet ({}): laptop → {}{} → router {} ({} ms) → internet 1.1.1.1 ({} ms) · DNS {} ({} ms) · web {} ms\n",
        r.waktu,
        if r.wifi_ssid.is_empty() { "kabel/jaringan".into() } else { format!("Wi-Fi '{}'", r.wifi_ssid) },
        r.sinyal.map(|x| format!(" sinyal {x}%")).unwrap_or_default(),
        if r.gateway.is_empty() { "-".into() } else { r.gateway.clone() },
        r.ping_gateway_ms.map(|m| m.to_string()).unwrap_or("gagal".into()),
        r.ping_internet_ms.map(|m| m.to_string()).unwrap_or("gagal".into()),
        if r.dns.is_empty() { "-".into() } else { r.dns.clone() },
        r.dns_ms.map(|m| m.to_string()).unwrap_or("gagal".into()),
        r.web_ms.map(|m| m.to_string()).unwrap_or("gagal".into()),
    );
    if !r.hop.is_empty() {
        s.push_str(&format!("Jalur (tracert): {}\n", r.hop.join(" → ")));
    }
    if !r.top_proses.is_empty() {
        s.push_str(&format!("Paling banyak koneksi: {}\n", r.top_proses.join(", ")));
    }
    s.push_str(&format!("Kesimpulan: {}", r.kesimpulan.join(" ")));
    s
}

pub async fn laporan(http: &reqwest::Client) -> String {
    teks(&periksa(http).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn dasar() -> Laporan {
        Laporan { gateway: "192.168.1.1".into(), ping_gateway_ms: Some(3), loss_gateway: Some(0), ping_internet_ms: Some(20), loss_internet: Some(0), dns_ms: Some(30), web_ms: Some(80), ..Default::default() }
    }
    #[test]
    fn diagnosis_rules() {
        assert!(simpulkan(&dasar())[0].contains("sehat"));
        let mut r = dasar();
        r.gateway.clear();
        assert!(simpulkan(&r)[0].contains("gateway"));
        let mut r = dasar();
        r.ping_internet_ms = None;
        r.loss_internet = Some(100);
        assert!(simpulkan(&r)[0].contains("ISP"));
        let mut r = dasar();
        r.dns_ms = None;
        assert!(simpulkan(&r)[0].contains("DNS"));
        let mut r = dasar();
        (r.loss_gateway, r.sinyal) = (Some(40), Some(30));
        assert!(simpulkan(&r)[0].contains("Sinyal"));
    }
}
