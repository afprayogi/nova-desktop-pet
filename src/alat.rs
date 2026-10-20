//! Alat kontrol komputer untuk para agen. Tindakan berisiko selalu butuh persetujuan Bos.
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::process::Command;

/// True = akses komputer dikunci (mis. web terbuka ke jaringan tanpa password).
pub static LOCKED: AtomicBool = AtomicBool::new(false);

/// True bila rencana terakhir dari plan_offline hanya hasil tebakan ML (bukan aturan pasti): Gemini lebih diutamakan.
pub static FROM_ML: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Serialize, Deserialize)]
pub struct Call {
    pub tool: String,
    pub arg: String,
}

pub enum Risk {
    Safe,
    Confirm(String),
    Block(String),
}

const DENY: &[&str] = &[
    "rm -rf /", "rm -rf ~", "rm -rf *", "rm -fr /", "mkfs", "format c:", "format d:", "del /s", "rd /s /q c:", "rmdir /s /q c:",
    "shutdown", "reboot", "reg delete", "diskpart", ":(){", "dd if=", "of=/dev/sd", "cipher /w", "bcdedit", "net user",
    "| sh", "| bash", "invoke-expression", "iex ", "downloadstring", "remove-item -recurse -force c:", "chmod -r 777 /",
];
const SENSITIVE: &[&str] = &[".ssh", "id_rsa", ".aws", ".env", "passwd", "shadow", "credentials", "config.json", "cookies", "login data"];

/// Perintah shell yang hanya membaca (tanpa pengalihan/penyambungan perintah) tidak perlu persetujuan.
fn shell_readonly(a: &str) -> bool {
    // Daftar putih: | || < > & ^ % ( ) ; ` $ dan baris baru bisa menyambung/menyisipkan perintah lain di cmd.exe.
    if !a.chars().all(|c| c.is_alphanumeric() || " ._:\\/=-\"*?,+@".contains(c)) {
        return false;
    }
    let a = a.trim();
    let first = a.split_whitespace().next().unwrap_or("");
    if ["dir", "ls", "type", "cat", "echo", "whoami", "hostname", "ipconfig", "ping", "tasklist", "systeminfo", "where", "ver", "date", "tree", "findstr", "get-childitem", "get-content", "get-process", "get-date", "netstat", "nslookup", "tracert"].contains(&first) {
        return true;
    }
    if first != "git" {
        return false;
    }
    // git hanya membaca: `branch -D`, `remote add`, `--output=file`, `--ext-diff` mengubah/menulis sesuatu.
    let mut it = a.split_whitespace().skip(1);
    match it.next() {
        Some("--version") | Some("status") => true,
        Some("branch") => it.all(|x| matches!(x, "--list" | "-a" | "-r" | "-v" | "-vv")),
        Some("remote") => it.all(|x| x == "-v"),
        Some("log") | Some("diff") | Some("show") => !a.contains("--output") && !a.contains("--ext-diff") && !a.contains("--textconv"),
        _ => false,
    }
}

/// Path yang akan dibuka berpotensi dieksekusi (exe/bat/lnk/vbs/js/msi...) atau bertipe tak dikenal.
fn buka_berisiko(arg: &str) -> bool {
    let a = arg.trim();
    if !a.contains(['/', '\\', ':', '.']) {
        return false; // nama aplikasi: dicari di menu Start/PATH
    }
    let p = std::path::Path::new(a);
    match p.extension().and_then(|e| e.to_str()) {
        Some(e) => !crate::indeks::SAFE_OPEN.contains(&e.to_lowercase().as_str()),
        None => !p.is_dir(),
    }
}

pub fn assess(c: &Call) -> Risk {
    let arg = c.arg.to_lowercase();
    if LOCKED.load(Ordering::Relaxed) && c.tool != "info" {
        return Risk::Block("akses komputer dikunci: server web terbuka tanpa KH_PASSWORD".into());
    }
    match c.tool.as_str() {
        "privasi" if arg.starts_with("hapus") => Risk::Confirm("menghapus riwayat aktivitas, memori, kesehatan, jurnal, dan log audit (pengaturan dan kunci tetap)".into()),
        "privasi" if arg.starts_with("ekspor") => Risk::Confirm("menyalin semua data pribadi NOVA (tanpa kunci/token) ke folder Dokumen/NOVA".into()),
        "cerdas" if arg.starts_with("layar") => Risk::Confirm("membaca layar dengan OCR lokal lalu merangkumnya (teks layar bisa dikirim ke model Gemini bila aktif)".into()),
        "cerdas" if arg.starts_with("memori hapus") || arg.starts_with("memori pudar") => Risk::Confirm("menghapus kenangan yang tersimpan".into()),
        "skrip" if arg.starts_with("jalankan") => Risk::Confirm("menjalankan skrip buatan (PowerShell/Python) di folder kerjanya; kodenya sudah kamu setujui, batas 60 detik".into()),
        "info" | "ps" | "health" | "kode" | "gambar" | "jurnal" | "harga" | "pantau" | "cuacapantau" | "gitringkas" | "halaman" | "peramban" | "internet" | "riwayat" | "csv" | "hitung" | "konversi" | "zona" | "ohm" | "fase3" | "hari" | "password" | "catat" | "catatan" | "cuaca" | "kurs" | "wiki" | "berpikir" | "terjemah" | "jadwal" | "otomatis" | "wifi" | "port" | "bukafile" | "cli" | "atur" | "winapp" | "waktu" | "web" | "tugas" | "musik" | "bukahasil" | "ingatan" | "email" | "cari" | "rapikan" | "apps" | "meme" | "baterai" | "ngobrol" | "berita" | "kepribadian" | "remind" | "agenda" | "habit" | "tidur" | "tanya" | "resume" | "selera" | "keahlian" | "kerja" | "jadwalkan" | "pemicu" | "skrip" | "kalk" | "saranproyek" | "cerdas" | "privasi" | "aturanberkas" | "pesan" => Risk::Safe,
        "pembaruan" if arg.trim() == "pasang" => Risk::Confirm("memasang pembaruan NOVA dari GitHub (diverifikasi tanda tangan); NOVA menutup diri sebentar".into()),
        "pembaruan" => Risk::Safe,
        "kurikulum" if SENSITIVE.iter().any(|s| arg.contains(s)) => Risk::Confirm("path tampak sensitif".into()),
        "graphify" if SENSITIVE.iter().any(|s| arg.contains(s)) => Risk::Confirm("path tampak sensitif".into()),
        "graphify" => Risk::Safe,
        "pelajari" if SENSITIVE.iter().any(|s| arg.contains(s)) => Risk::Confirm("path tampak sensitif".into()),
        "pelajari" | "kurikulum" => Risk::Safe,
        "kirim" => Risk::Confirm("mengirim email ke alamat Bos sendiri".into()),
        "otak" | "grafik" | "bantuan" | "dns" | "diagnosa" | "validasi" | "pomodoro" => Risk::Safe,
        "hash" if SENSITIVE.iter().any(|x| arg.contains(x)) => Risk::Confirm("path tampak sensitif".into()),
        "hash" => Risk::Safe,
        "cariisi" if SENSITIVE.iter().any(|x| arg.contains(x)) => Risk::Confirm("path tampak sensitif".into()),
        "cariisi" | "github" | "tunggu" => Risk::Safe,
        "kompres" if SENSITIVE.iter().any(|x| arg.contains(x)) => Risk::Block("path sensitif tidak dikompres".into()),
        "kompres" => Risk::Confirm(format!("membuat berkas zip dari {}", c.arg.trim())),
        "paket" if arg.contains("tangkap") => Risk::Confirm("menangkap paket jaringan ±15 detik (Windows akan meminta UAC)".into()),
        "paket" => Risk::Safe,
        "daya" if matches!(arg.trim(), "kunci" | "batal") => Risk::Safe,
        "daya" => Risk::Confirm(format!("daya laptop: {}", arg.trim())),
        "urungkan" => Risk::Confirm("mengembalikan perubahan berkas terakhir (tulis/rapikan)".into()),
        "layar" => Risk::Safe, // hanya membaca (OCR lokal, tidak disimpan)
        "pdf" | "ringkas" if SENSITIVE.iter().any(|s| arg.contains(s)) => Risk::Confirm("path tampak sensitif".into()),
        "pdf" | "ringkas" => Risk::Safe,
        "fetch" if arg.starts_with("http://") || arg.starts_with("https://") => Risk::Safe,
        "fetch" => Risk::Block("fetch hanya menerima URL http(s)".into()),
        "ls" | "read" => {
            if SENSITIVE.iter().any(|s| arg.contains(s)) {
                Risk::Confirm("path tampak sensitif (kunci/rahasia/konfigurasi)".into())
            } else {
                Risk::Safe
            }
        }
        "shell" => match DENY.iter().find(|d| arg.contains(*d)) {
            Some(d) => Risk::Block(format!("perintah berbahaya terdeteksi: '{d}'")),
            None if SENSITIVE.iter().any(|s| arg.contains(s)) => Risk::Confirm("perintah menyentuh jalur sensitif (kunci/rahasia/konfigurasi)".into()),
            None if shell_readonly(&arg) => Risk::Safe,
            None => Risk::Confirm("menjalankan perintah yang bisa mengubah komputer Bos (edit/tambah/hapus)".into()),
        },
        "bersih" => Risk::Confirm("menghapus cache yang bisa diunduh ulang (Temp lama, npm, pip, Hugging Face)".into()),
        "write" => Risk::Confirm("menulis/menimpa file di komputer Bos".into()),
        "open" => {
            let url = c.arg.starts_with("https://") || c.arg.starts_with("http://");
            if c.arg.chars().any(|ch| if url { "&|<>^\"'\n ".contains(ch) } else { "&|<>^\"%'\n".contains(ch) }) {
                Risk::Block("target open memuat karakter terlarang".into())
            } else {
                if !url && buka_berisiko(&c.arg) {
                    Risk::Confirm("membuka berkas yang bisa dieksekusi atau bertipe tak dikenal".into())
                } else {
                    Risk::Safe
                }
            }
        }
        t => Risk::Block(format!("alat tidak dikenal: {t}")),
    }
}

/// "https://www.youtube.com/results?search_query=spongebob" → "Youtube: spongebob" (yang dibuka terlihat jelas di obrolan).
fn tampil_url(u: &str) -> String {
    let host = u.split("://").nth(1).unwrap_or(u).split('/').next().unwrap_or("").trim_start_matches("www.");
    let q = u.split(['?', '&']).skip(1).find_map(|p| ["q=", "search_query=", "search=", "query=", "term=", "keyword=", "text=", "ss="].iter().find_map(|k| p.strip_prefix(k)));
    match q {
        Some(q) => {
            let nama = host.split('.').rev().nth(1).unwrap_or(host);
            let mut c = nama.chars();
            format!("{}{}: {}", c.next().map(|f| f.to_uppercase().to_string()).unwrap_or_default(), c.as_str(), urlencoding::decode(q).map(|d| d.into_owned()).unwrap_or_default())
        }
        None => host.to_string(),
    }
}

fn publik(ip: &std::net::IpAddr) -> bool {
    use std::net::IpAddr;
    match ip {
        IpAddr::V4(v) => {
            let o = v.octets();
            !(v.is_loopback() || v.is_private() || v.is_link_local() || v.is_unspecified() || v.is_broadcast() || (o[0] == 100 && (64..128).contains(&o[1])))
        }
        IpAddr::V6(v) => {
            let s = v.segments();
            !(v.is_loopback() || v.is_unspecified() || (s[0] & 0xfe00) == 0xfc00 || (s[0] & 0xffc0) == 0xfe80 || v.to_ipv4_mapped().is_some_and(|x| !publik(&IpAddr::V4(x))))
        }
    }
}

/// Alat web hanya untuk internet publik: tolak localhost, jaringan privat, dan link-local (API NOVA sendiri, router, metadata).
// shortcut: DNS dicek sebelum permintaan; DNS rebinding antar-pemeriksaan tidak ditangani. Naikkan bila NOVA dipasang di server.
pub async fn url_publik(url: &str) -> Result<(), String> {
    use std::net::IpAddr;
    let u = reqwest::Url::parse(url.trim()).map_err(|_| "URL tidak valid".to_string())?;
    if !matches!(u.scheme(), "http" | "https") {
        return Err("hanya URL http(s)".into());
    }
    let host = u.host_str().ok_or("URL tanpa host")?.trim_matches(['[', ']']).to_string();
    let ips: Vec<IpAddr> = match host.parse::<IpAddr>() {
        Ok(ip) => vec![ip],
        Err(_) if host == "localhost" || host.ends_with(".localhost") => return Err("alamat lokal tidak diizinkan".into()),
        Err(_) => tokio::net::lookup_host((host.as_str(), u.port_or_known_default().unwrap_or(80))).await.map_err(|e| format!("DNS gagal: {e}"))?.map(|a| a.ip()).collect(),
    };
    if ips.is_empty() || ips.iter().any(|ip| !publik(ip)) {
        return Err("alamat lokal/privat tidak diizinkan untuk alat web".into());
    }
    Ok(())
}

/// GET dengan redirect manual (tiap lompatan dicek publik) dan isi dibatasi 1 MB agar RAM laptop aman.
pub async fn ambil_publik(url: &str) -> Result<String, String> {
    const MAKS: usize = 1_048_576;
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(15))
        .user_agent("Mozilla/5.0 NOVA")
        .build()
        .map_err(|e| e.to_string())?;
    let mut cur = url.trim().to_string();
    for _ in 0..5 {
        url_publik(&cur).await?;
        let mut r = http.get(&cur).send().await.map_err(|e| e.to_string())?;
        if r.status().is_redirection() {
            let loc = r.headers().get("location").and_then(|v| v.to_str().ok()).ok_or("redirect tanpa tujuan")?;
            cur = reqwest::Url::parse(&cur).and_then(|b| b.join(loc)).map_err(|e| e.to_string())?.to_string();
            continue;
        }
        let mut buf: Vec<u8> = Vec::new();
        while let Some(ch) = r.chunk().await.map_err(|e| e.to_string())? {
            buf.extend_from_slice(&ch);
            if buf.len() >= MAKS {
                break;
            }
        }
        return Ok(String::from_utf8_lossy(&buf).into_owned());
    }
    Err("terlalu banyak redirect".into())
}

fn cut(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…[dipotong]", s.chars().take(n).collect::<String>())
    }
}

async fn sh(cmd: &str) -> String {
    let mut c = if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.args(["/C", cmd]);
        c
    } else {
        let mut c = Command::new("sh");
        c.args(["-c", cmd]);
        c
    };
    c.kill_on_drop(true).stdin(std::process::Stdio::null());
    match tokio::time::timeout(Duration::from_secs(30), c.output()).await {
        Err(_) => "timeout 30 detik, proses dihentikan".into(),
        Ok(Err(e)) => format!("gagal menjalankan: {e}"),
        Ok(Ok(o)) => {
            let mut t = String::from_utf8_lossy(&o.stdout).to_string();
            let e = String::from_utf8_lossy(&o.stderr);
            if !e.trim().is_empty() {
                t.push_str("\n[stderr] ");
                t.push_str(&e);
            }
            format!("[exit {}] {}", o.status.code().unwrap_or(-1), t.trim())
        }
    }
}

fn strip_tags(html: &str) -> String {
    let (mut out, mut tag) = (String::new(), false);
    for ch in html.chars() {
        match ch {
            '<' => tag = true,
            '>' => {
                tag = false;
                out.push(' ');
            }
            c if !tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Jalankan alat (risiko sudah dinilai/disetujui oleh pemanggil).
pub async fn run(c: &Call, http: &reqwest::Client) -> String {
    let arg = c.arg.trim();
    let out = match c.tool.as_str() {
        "info" => format!(
            "os={} arch={} cwd={} user={} waktu_unix={}",
            std::env::consts::OS,
            std::env::consts::ARCH,
            std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_default(),
            std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_default(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
        ),
        "health" => crate::monitor::summary(),
        "pdf" | "ringkas" => {
            let p = arg.to_string();
            match tokio::task::spawn_blocking(move || read_doc(&p)).await {
                Ok(Ok(t)) if c.tool == "ringkas" => summarize(&t, 6),
                Ok(Ok(t)) => t.chars().take(6000).collect(),
                Ok(Err(e)) => format!("gagal: {e}"),
                Err(_) => "gagal: dokumen tidak bisa dibaca".into(),
            }
        }
        "layar" => match tokio::task::spawn_blocking(crate::layar::read_screen).await {
            Ok(Ok(t)) if t.trim().is_empty() => "layar tidak berisi teks yang terbaca".into(),
            Ok(Ok(t)) if arg.contains("ringkas") => summarize(&t, 5),
            Ok(Ok(t)) => t.chars().take(1500).collect(),
            Ok(Err(e)) => format!("gagal: {e}"),
            Err(_) => "gagal membaca layar".into(),
        },
        "baterai" => crate::baterai::report(),
        "email" => crate::google::summary(http).await,
        "ingatan" => {
            let a = arg.to_string();
            tokio::task::spawn_blocking(move || crate::graf::about(&a)).await.unwrap_or_else(|_| "gagal membaca graf".into())
        }
        "graphify" => {
            let a = arg.trim().trim_matches('"').to_string();
            tokio::task::spawn_blocking(move || {
                let p = std::path::Path::new(&a);
                if p.is_dir() { crate::graf::graphify_folder(p) } else { format!("Folder tidak ditemukan: {a}") }
            })
            .await
            .unwrap_or_else(|_| "gagal memetakan folder".into())
        }
        "cari" => {
            let a = arg.to_string();
            let r = tokio::task::spawn_blocking(move || {
                let r = crate::berkas::cari(&a);
                if r.starts_with("🔎 Ketemu") {
                    crate::indeks::reveal(1); // langsung tunjukkan foldernya
                }
                r
            })
            .await
            .unwrap_or_else(|_| "pencarian gagal".into());
            r
        }
        "atur" => {
            let a = arg.to_string();
            tokio::task::spawn_blocking(move || crate::sistem::atur(&a)).await.unwrap_or_else(|_| "gagal mengatur".into())
        }
        "waktu" => crate::sistem::waktu_text(),
        "bukapath" => {
            let a = arg.to_string();
            tokio::task::spawn_blocking(move || if crate::seret::is_dropped(&a) { crate::seret::open_path(&a) } else { "Jalur itu bukan berkas yang baru Bos seret/pilih, jadi tidak kubuka.".into() }).await.unwrap_or_else(|_| "gagal membuka".into())
        }
        "jadwal" if arg.contains("sekarang") => tokio::task::spawn_blocking(crate::rencana::jika_tidur_sekarang).await.unwrap_or_else(|_| "gagal menghitung".into()),
        "jadwal" => tokio::task::spawn_blocking(crate::rencana::plan).await.unwrap_or_else(|_| "gagal menghitung rencana tidur".into()),
        "otomatis" => crate::sistem::auto_cmd(arg),
        "wifi" => tokio::task::spawn_blocking(crate::sistem::wifi).await.unwrap_or_else(|_| "gagal membaca Wi-Fi".into()),
        "port" => tokio::task::spawn_blocking(crate::makro::ports).await.unwrap_or_else(|_| "gagal membaca port".into()),
        "bukafile" => {
            let a = arg.to_string();
            tokio::task::spawn_blocking(move || {
                let r = crate::berkas::cari(&a);
                if r.starts_with("🔎 Ketemu") {
                    format!("{}\n{}", crate::indeks::open_file(1), r.lines().skip(1).take(3).collect::<Vec<_>>().join("\n"))
                } else {
                    r
                }
            })
            .await
            .unwrap_or_else(|_| "pencarian gagal".into())
        }
        "cli" => {
            let a = arg.to_string();
            tokio::task::spawn_blocking(move || crate::cli::help(&a)).await.unwrap_or_else(|_| "gagal membaca bantuan".into())
        }
        "web" => crate::gemini::web(http, &crate::rawat::perluas(arg)).await,
        "gambar" => crate::gambar::cari(http, arg).await,
        "harga" => crate::harga::harga(http, arg).await,
        "pantau" => crate::harga::pantau(http, arg).await,
        "cuacapantau" => crate::harga::cuaca_pantau(arg),
        "gitringkas" => crate::ekstra::git_ringkas(http, arg).await,
        "halaman" => crate::ekstra::halaman(http, arg).await,
        "peramban" => crate::peramban::baca(arg).await,
        "internet" => crate::internet::laporan(http).await,
        "riwayat" => tokio::task::spawn_blocking(crate::riwayat::ringkas).await.unwrap_or_default(),
        "daya" => crate::daya::jalankan(arg),
        "cariisi" => {
            let a = arg.to_string();
            tokio::task::spawn_blocking(move || crate::alatbaru::cari_isi(&a)).await.unwrap_or_default()
        }
        "github" => crate::alatbaru::github(http, arg).await,
        "tunggu" => crate::alatbaru::tunggu(arg).await,
        "kompres" => {
            let a = arg.to_string();
            tokio::task::spawn_blocking(move || crate::alatbaru::kompres(&a)).await.unwrap_or_default()
        }
        "grafik" => tokio::task::spawn_blocking(crate::grafik::hari_ini).await.unwrap_or_default(),
        "bantuan" => {
            // bantuan alat: cocokkan kata di argumen dengan nama/alias alat di katalog (alat.json), lalu tampilkan sintaks dan contohnya
            let l = arg.to_lowercase();
            let kat = crate::data::lapis("alat.json", crate::registry::BAWAAN);
            let hit = kat["alat"].as_array().and_then(|a| a.iter().find(|t| {
                let n = t["nama"].as_str().unwrap_or("");
                !n.is_empty() && l.split(|c: char| !c.is_alphanumeric() && c != '/').any(|w| w == n || t["alias"].as_array().is_some_and(|al| al.iter().any(|x| x.as_str() == Some(w))))
            }));
            match hit {
                Some(t) => {
                    let contoh: Vec<&str> = t["contoh"].as_array().into_iter().flatten().filter_map(|x| x["perintah"].as_str()).take(3).collect();
                    format!("🛠 {} {}: {}
Cara memicu: {}
Contoh: {}
Risiko: {}{}", t["nama"].as_str().unwrap_or(""), t["arg"].as_str().unwrap_or(""), t["deskripsi"].as_str().unwrap_or(""), t["pemicu_bahasa_alami"].as_array().into_iter().flatten().filter_map(|x| x.as_str()).take(3).collect::<Vec<_>>().join("; "), contoh.join("; "), t["perilaku"]["tingkat_risiko"].as_str().unwrap_or("-"), if t["perilaku"]["butuh_persetujuan"].as_bool() == Some(true) { " (butuh persetujuan Bos)" } else { "" })
                }
                None => crate::cli::find(arg),
            }
        }
        "diagnosa" => crate::internet::laporan(http).await,
        "pomodoro" => crate::widget::aksi(&serde_json::json!({"aksi": "fokus_mulai", "menit": arg.split_whitespace().find_map(|w| w.parse::<u32>().ok()).unwrap_or(25).clamp(5, 120)})),
        "dns" => {
            let host = arg.trim().trim_start_matches("https://").trim_start_matches("http://").split('/').next().unwrap_or("").to_string();
            tokio::task::spawn_blocking(move || {
                use std::net::ToSocketAddrs;
                if host.is_empty() || host.len() > 253 || !host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') {
                    return "Sebut nama domain yang valid, mis. dns example.com".to_string();
                }
                match (host.as_str(), 80).to_socket_addrs() {
                    Ok(a) => format!("{host} → {}", a.map(|x| x.ip().to_string()).collect::<std::collections::BTreeSet<_>>().into_iter().collect::<Vec<_>>().join(", ")),
                    Err(e) => format!("DNS gagal untuk {host}: {e}"),
                }
            })
            .await
            .unwrap_or_default()
        }
        "validasi" => {
            let a = arg.trim().trim_matches('"').to_string();
            tokio::task::spawn_blocking(move || {
                let teks = if std::path::Path::new(&a).is_file() { std::fs::read_to_string(&a).unwrap_or_default() } else { a.clone() };
                match serde_json::from_str::<serde_json::Value>(&teks) {
                    Ok(_) => "JSON valid ✔".to_string(),
                    Err(e) => format!("JSON tidak valid: {e}"),
                }
            })
            .await
            .unwrap_or_default()
        }
        "hash" => {
            let a = arg.trim().trim_matches('"').to_string();
            tokio::task::spawn_blocking(move || {
                use sha2::{Digest, Sha256};
                let (nama, bytes) = if std::path::Path::new(&a).is_file() {
                    match std::fs::metadata(&a) {
                        Ok(m) if m.len() > 500_000_000 => return "Berkas terlalu besar (>500 MB).".to_string(),
                        _ => (a.clone(), std::fs::read(&a).unwrap_or_default()),
                    }
                } else {
                    ("teks".to_string(), a.clone().into_bytes())
                };
                format!("SHA-256 ({nama}): {}", Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect::<String>())
            })
            .await
            .unwrap_or_default()
        }
        "otak" => tokio::task::spawn_blocking(crate::otak::teks).await.unwrap_or_default(),
        "paket" if arg.contains("tangkap") => crate::paket::tangkap(15).await,
        "paket" => crate::paket::teks_monitor(&crate::paket::monitor().await),
        "kirim" => crate::surel::kirim(arg).await,
        "csv" => crate::ekstra::csv(arg),
        "hitung" => crate::perkakas::hitung(arg),
        "konversi" => crate::perkakas::konversi(arg),
        "zona" => crate::perkakas::zona(arg),
        "ohm" => crate::perkakas::ohm(arg),
        "fase3" => crate::perkakas::fase3(arg),
        "hari" => crate::perkakas::hari(arg),
        "password" => crate::perkakas::password(arg),
        "catat" => crate::perkakas::catat(arg),
        "catatan" => crate::perkakas::catatan(arg),
        "cuaca" => crate::daring::cuaca(http, arg).await,
        "kurs" => crate::daring::kurs(http, arg).await,
        "wiki" => crate::daring::wiki(http, arg).await,
        "berpikir" => crate::daring::berpikir(http, arg).await,
        "terjemah" => crate::daring::terjemah(http, arg).await,
        "jurnal" => crate::ilmiah::cari(http, arg).await,
        "bersih" => tokio::task::spawn_blocking(crate::sistem::bersih_aman).await.unwrap_or_else(|_| "gagal membersihkan".into()),
        "kode" => {
            let a = arg.to_string();
            tokio::task::spawn_blocking(move || crate::sistem::kode(&a)).await.unwrap_or_else(|_| "gagal membuka VS Code".into())
        }
        "winapp" => {
            let a = arg.to_string();
            tokio::task::spawn_blocking(move || crate::sistem::winapp(&a)).await.unwrap_or_else(|_| "gagal membuka aplikasi".into())
        }
        "tugas" => crate::tugas::set(arg),
        "musik" => tokio::task::spawn_blocking(crate::musik::play).await.unwrap_or_else(|_| "gagal memutar musik".into()),
        "bukahasil" => crate::indeks::reveal(arg.chars().filter(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(1)),
        "rapikan" => {
            let a = arg.to_string();
            tokio::task::spawn_blocking(move || crate::berkas::rapikan(&a)).await.unwrap_or_else(|_| "gagal menyusun rencana".into())
        }
        "apps" => tokio::task::spawn_blocking(crate::berkas::apps).await.unwrap_or_else(|_| "gagal memeriksa aplikasi".into()),
        "urungkan" => {
            // urungkan aksi TERBARU: penulisan berkas (jurnal) atau rapikan, mana yang lebih baru
            if crate::jurnal::latest_ts() > crate::berkas::undo_mtime() {
                crate::jurnal::undo_last().unwrap_or_else(|| "Tidak ada yang bisa diurungkan.".into())
            } else {
                crate::berkas::urungkan()
            }
        }
        "meme" => crate::meme::meme(arg, arg.len() + chrono::Timelike::second(&chrono::Local::now()) as usize),
        "berita" => crate::berita::headlines(http, arg, false).await.0,
        "kepribadian" => crate::bandit::report(),
        "keahlian" => crate::keahlian::jalankan(http, arg).await,
        "jadwalkan" => crate::penjadwal::jalankan(http, arg).await,
        "pemicu" => crate::pemicu::jalankan(http, arg).await,
        "skrip" => crate::skrip::jalankan(http, arg).await,
        "kalk" => crate::engineer::kalk(arg),
        "cerdas" => crate::cerdas::jalankan(http, arg).await,
        "privasi" => crate::audit::jalankan(arg),
        "aturanberkas" => {
            let a = arg.to_string();
            tokio::task::spawn_blocking(move || crate::berkas::aturan_berkas(&a)).await.unwrap_or_else(|_| "gagal".into())
        }
        "saranproyek" => {
            tokio::task::spawn_blocking(crate::engineer::saran_proyek).await.unwrap_or_else(|_| "gagal menyusun saran".into())
        }
        "pesan" => {
            crate::notify::kirim("NOVA", arg);
            format!("📣 {arg}")
        }
        "kerja" => {
            let a = arg.to_string();
            tokio::task::spawn_blocking(move || crate::kerja::jalankan(&a)).await.unwrap_or_else(|_| "gagal menyusun rekap".into())
        }
        "pembaruan" => crate::pembaruan::jalankan(http, arg).await,
        "resume" => tokio::task::spawn_blocking(crate::kita::resume).await.unwrap_or_else(|_| "gagal menyusun resume".into()),
        "selera" => tokio::task::spawn_blocking(crate::kita::selera).await.unwrap_or_else(|_| "gagal membaca selera".into()),
        "kurikulum" => {
            let a = arg.to_string();
            tokio::task::spawn_blocking(move || crate::kurikulum::run(&a)).await.unwrap_or_else(|_| "gagal membuat kurikulum".into())
        }
        "ngobrol" => ngobrol(arg),
        "remind" => crate::assistant::add_reminder(arg),
        "agenda" | "habit" | "tidur" => crate::assistant::report(&c.tool),
        "tanya" => {
            let out: String = 't: {
            let nv = crate::vektor::tanya(http, arg).await;
            let mut r = crate::teknik::tanya_v(arg, nv.as_deref());
            // Buku berbahasa Inggris: pertanyaan Indonesia juga dicari dalam bentuk Inggris (pencarian berbasis kata tidak lintas bahasa).
            if arg.split_whitespace().count() >= 3 && !crate::terjemah::kemungkinan_inggris(arg) && !crate::teknik::buku_terpelajari().is_empty() {
                if let Ok((en, _, _)) = crate::terjemah::terjemahkan(http, arg, Some("en")).await {
                    let r2 = crate::teknik::tanya(&en);
                    if r2.contains(".pdf]") || r2.contains(".md]") || r2.contains(".txt]") {
                        r = r2;
                    }
                }
            }
            // Sumber berbahasa Inggris (mis. buku teknik): sertakan terjemahan Indonesia di depan.
            if crate::terjemah::kemungkinan_inggris(&r) {
                let awal: String = r.chars().take(1500).collect();
                if let Ok((t, _, _)) = crate::terjemah::terjemahkan(http, &awal, Some("id")).await {
                    break 't cut(&format!("🌐 {t}

— teks asli —
{r}"), 3000);
                }
            }
            r
            };
            if crate::teknik::memuaskan(arg, &out) {
                crate::teknik::catat_eskalasi(false);
                crate::teknik::batal_nilai();
                out
            } else {
                // Lokal belum cukup: Gemini (cadangan Llama) menjawab dengan bahan dari otak, hasilnya disimpan agar lain kali dijawab lokal.
                crate::teknik::catat_eskalasi(true);
                match crate::teknik::naik(http, arg, &out).await {
                    Some(j) => cut(&j, 3000),
                    None => out,
                }
            }
        }
        "pelajari" => {
            let p = arg.to_string();
            tokio::task::spawn_blocking(move || crate::teknik::pelajari(&p)).await.unwrap_or_else(|_| "gagal".into())
        }
        "ps" => sh(if cfg!(windows) { "tasklist" } else { "ps aux | head -50" }).await,
        "ls" => match std::fs::read_dir(if arg.is_empty() { "." } else { arg }) {
            Ok(rd) => rd
                .flatten()
                .take(200)
                .map(|e| {
                    let d = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    format!("{}{}", e.file_name().to_string_lossy(), if d { "/" } else { "" })
                })
                .collect::<Vec<_>>()
                .join("\n"),
            Err(e) => format!("gagal: {e}"),
        },
        "read" => match std::fs::read(arg) {
            Ok(b) => String::from_utf8_lossy(&b[..b.len().min(20_000)]).to_string(),
            Err(e) => format!("gagal: {e}"),
        },
        "fetch" => match ambil_publik(arg).await {
            Ok(t) => strip_tags(&t),
            Err(e) => format!("gagal: {e}"),
        },
        "shell" => sh(arg).await,
        "write" => {
            let (path, body) = c.arg.split_once('\n').unwrap_or((arg, ""));
            if body.len() > 100_000 {
                "gagal: isi terlalu besar (>100KB)".into()
            } else {
                crate::jurnal::before_write(path.trim());
                match std::fs::write(path.trim(), body) {
                    Ok(_) => format!("ditulis {} byte ke {}", body.len(), path.trim()),
                    Err(e) => format!("gagal: {e}"),
                }
            }
        }
        "open" if arg.contains("wikipedia.org/w/index.php?search=") => {
            // Diminta mencari di Wikipedia: artikelnya juga dimasukkan ke otak, lalu halamannya dibuka.
            let q = urlencoding::decode(arg.split("search=").nth(1).unwrap_or("")).map(|c| c.into_owned()).unwrap_or_default();
            let r = crate::wikipedia::cari_simpan(http, &q).await;
            crate::musik::buka_url(arg);
            format!("{r} (halaman dibuka di browser)")
        }
        "open" if arg.starts_with("https://www.youtube.com/results?search_query=") => {
            let q = urlencoding::decode(arg.trim_start_matches("https://www.youtube.com/results?search_query=")).map(|c| c.into_owned()).unwrap_or_default();
            crate::musik::catat(&q);
            // Pencarian musik: langsung putar video teratas (bukan berhenti di daftar hasil).
            let musik = q.to_lowercase().split_whitespace().any(|w| ["lagu", "musik", "music", "lofi", "playlist"].contains(&w));
            if musik {
                let q2 = q.clone();
                let _ = tokio::task::spawn_blocking(move || crate::musik::putar(&q2)).await;
                return format!("memutar di YouTube: {q}");
            }
            crate::musik::buka_url(arg);
            format!("dibuka: {}", tampil_url(arg))
        }
        "open" if arg.starts_with("https://") || arg.starts_with("http://") => {
            // URL dibuka lewat penangan protokol Windows tanpa shell (aman dari injeksi perintah)
            let a = arg.to_string();
            let mut c = std::process::Command::new("rundll32");
            c.args(["url.dll,FileProtocolHandler", &a]);
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                c.creation_flags(0x0800_0000);
            }
            let _ = c.spawn();
            format!("dibuka: {}", tampil_url(arg))
        }
        "open" if !arg.contains(['/', '\\', ':', '.']) && !arg.is_empty() => {
            // Nama aplikasi (bukan URL/path): cari di indeks menu Start, lalu PATH; laporkan jujur bila tidak ada.
            let q = arg.to_string();
            tokio::task::spawn_blocking(move || {
                if let Some(a) = crate::aplikasi::cari(&q) {
                    return if crate::aplikasi::buka(&a) { format!("dibuka: {}", a.nama) } else { format!("gagal membuka {}", a.nama) };
                }
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    let ada = std::process::Command::new("where").arg(&q).creation_flags(0x0800_0000).output().map(|o| o.status.success()).unwrap_or(false);
                    if ada {
                        let mut c = std::process::Command::new("cmd");
                        c.raw_arg(format!("/C start \"\" \"{q}\"")).creation_flags(0x0800_0000);
                        let _ = c.spawn();
                        return format!("dibuka: {q}");
                    }
                }
                format!("Tidak menemukan aplikasi '{q}' di laptop ini. Ketik /aplikasi {q} untuk melihat yang terpasang.")
            })
            .await
            .unwrap_or_else(|_| "gagal membuka aplikasi".into())
        }
        "open" => {
            #[cfg(windows)]
            {
                // raw_arg: cmd.exe butuh kutip apa adanya (args() akan meng-escape jadi \"\" dan memunculkan galat '\\').
                use std::os::windows::process::CommandExt;
                let a = arg.to_string();
                let _ = tokio::task::spawn_blocking(move || {
                    let mut c = std::process::Command::new("cmd");
                    c.raw_arg(format!("/C start \"\" \"{a}\"")).creation_flags(0x0800_0000);
                    c.spawn()
                })
                .await;
                return cut(&format!("dibuka: {}", tampil_url(arg)), 3000);
            }
            #[allow(unreachable_code)]
            let cmd = if cfg!(windows) {
                format!("start \"\" \"{arg}\"")
            } else if cfg!(target_os = "macos") {
                format!("open '{arg}'")
            } else {
                format!("xdg-open '{arg}'")
            };
            sh(&cmd).await;
            format!("dibuka: {}", tampil_url(arg))
        }
        t => format!("alat tidak dikenal: {t}"),
    };
    crate::audit::catat(c, &out);
    cut(&out, 3000)
}

/// Ambil objek JSON pertama dari balasan model (model kecil sering menambah teks di sekitarnya).
pub fn parse_json(t: &str) -> Option<serde_json::Value> {
    let (a, b) = (t.find('{')?, t.rfind('}')?);
    serde_json::from_str(t.get(a..=b)?).ok()
}

/// Teks dari PDF (atau file teks biasa), maks 300.000 karakter (aman untuk RAM kecil).
pub(crate) fn read_doc(path: &str) -> Result<String, String> {
    read_doc_max(path, 300_000)
}

/// PDF per halaman lewat lopdf: halaman yang gagal dilewati (PDF tebal/aneh sering membuat pdf-extract panik di satu halaman).
fn pdf_per_halaman(path: &str, max: usize) -> Result<String, String> {
    // Pengurai toleran sendiri dulu (peta ToUnicode rusak tidak menggagalkan halaman); lopdf bawaan sebagai cadangan.
    if let Ok(t) = crate::pdfteks::dokumen(path, max) {
        if t.trim().len() > 200 {
            return Ok(t);
        }
    }
    let doc = lopdf::Document::load(path).map_err(|e| e.to_string())?;
    let mut out = String::new();
    for n in doc.get_pages().keys().copied().collect::<Vec<u32>>() {
        if out.chars().count() >= max {
            break;
        }
        if let Ok(Ok(t)) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| doc.extract_text(&[n]))) {
            out.push_str(t.trim());
            out.push_str("\n\n");
        }
    }
    Ok(out)
}

/// Seperti `read_doc` tetapi dengan batas karakter sendiri (buku tebal perlu jauh lebih dari 300.000).
pub(crate) fn read_doc_max(path: &str, max: usize) -> Result<String, String> {
    let path = path.trim();
    let lower = path.to_ascii_lowercase();
    let text = if lower.ends_with(".pdf") {
        let p = path.to_string();
        let utama = std::panic::catch_unwind(move || pdf_extract::extract_text(&p)).ok().and_then(|r| r.ok()).filter(|t| t.trim().len() > 200);
        match utama {
            Some(t) => t,
            None => pdf_per_halaman(path, max)?,
        }
    } else {
        String::from_utf8_lossy(&std::fs::read(path).map_err(|e| e.to_string())?).into_owned()
    };
    if text.trim().is_empty() {
        return Err("tidak ada teks (PDF hasil scan/gambar butuh OCR)".into());
    }
    Ok(text.chars().take(max).collect())
}

const STOP: &[&str] = &[
    "yang", "dan", "di", "ke", "dari", "ini", "itu", "untuk", "pada", "dengan", "adalah", "dalam", "atau", "juga", "akan", "oleh", "sebagai",
    "the", "of", "and", "to", "in", "a", "is", "for", "that", "on", "with", "as", "by", "are", "be", "this", "an", "it", "or", "at", "from",
];

/// Ringkasan ekstraktif: pilih kalimat berskor frekuensi kata tertinggi, urut sesuai dokumen. Tanpa model.
pub fn summarize(text: &str, n: usize) -> String {
    use std::collections::HashMap;
    let sents: Vec<String> = text
        .split(|c| matches!(c, '.' | '!' | '?' | '\n'))
        .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|s| s.chars().count() > 30)
        .collect();
    let words = |s: &str| -> Vec<String> {
        s.split(|c: char| !c.is_alphanumeric()).map(|w| w.to_lowercase()).filter(|w| w.len() > 2 && !STOP.contains(&w.as_str())).collect()
    };
    let mut freq: HashMap<String, f32> = HashMap::new();
    for s in &sents {
        for w in words(s) {
            *freq.entry(w).or_default() += 1.0;
        }
    }
    let mut scored: Vec<(usize, f32)> = sents
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let w = words(s);
            let sc = w.iter().map(|x| freq[x]).sum::<f32>() / (w.len().max(1) as f32).sqrt();
            (i, sc)
        })
        .collect();
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut pick: Vec<usize> = scored.into_iter().take(n).map(|x| x.0).collect();
    pick.sort();
    let body = pick.iter().map(|&i| format!("- {}", sents[i].chars().take(300).collect::<String>())).collect::<Vec<_>>().join("\n");
    format!("({} kata, {} kalimat; ringkasan ekstraktif)\n{}", text.split_whitespace().count(), sents.len(), body)
}

/// Rencana tanpa model: kata kunci -> alat. None jika ada bagian yang tidak dipahami.
pub fn plan_offline(goal: &str) -> Option<Vec<Call>> {
    FROM_ML.store(false, Ordering::Relaxed);
    // Permintaan membuat keahlian berisi uraian panjang ("... lalu ... lalu ..."): jangan dipecah per "lalu".
    let kecil = goal.trim().to_lowercase();
    for p in ["buat keahlian", "buatkan keahlian", "tambah keahlian", "buat skill", "pelajari cara", "ajari dirimu", "kembangkan dirimu"] {
        if kecil.starts_with(p) && kecil.len() > p.len() + 3 {
            return Some(vec![Call { tool: "keahlian".into(), arg: format!("buat {}", goal.trim()[p.len()..].trim()) }]);
        }
    }
    // Permintaan penjadwalan berisi uraian ("... lalu ..."): jangan dipecah per "lalu".
    if kecil == "buat jadwal ini" {
        return Some(vec![Call { tool: "jadwalkan".into(), arg: "simpan".into() }]);
    }
    if kecil == "batal jadwal ini" {
        return Some(vec![Call { tool: "jadwalkan".into(), arg: "batal".into() }]);
    }
    if kecil == "simpan skrip" || kecil == "batal skrip" {
        return Some(vec![Call { tool: "skrip".into(), arg: kecil.trim_end_matches(" skrip").into() }]);
    }
    for p in ["buat skrip", "buatkan skrip", "tulis skrip", "tuliskan skrip"] {
        if kecil.starts_with(p) && kecil.len() > p.len() + 3 {
            return Some(vec![Call { tool: "skrip".into(), arg: format!("buat {}", goal.trim()[p.len()..].trim()) }]);
        }
    }
    if kecil == "buat pemicu ini" {
        return Some(vec![Call { tool: "pemicu".into(), arg: "simpan".into() }]);
    }
    if kecil == "batal pemicu ini" {
        return Some(vec![Call { tool: "pemicu".into(), arg: "batal".into() }]);
    }
    if let Some(n) = kecil.strip_prefix("jadwalkan templat ").or_else(|| kecil.strip_prefix("pemicu templat ")) {
        let alat = if kecil.starts_with("pemicu") { "pemicu" } else { "jadwalkan" };
        return Some(vec![Call { tool: alat.into(), arg: format!("templat {}", n.trim()) }]);
    }
    for p in ["buat pemicu", "buatkan pemicu", "tambah pemicu", "pasang pemicu"] {
        if kecil.starts_with(p) && kecil.len() > p.len() + 3 {
            return Some(vec![Call { tool: "pemicu".into(), arg: format!("buat {}", goal.trim()[p.len()..].trim()) }]);
        }
    }
    if ["kalau ", "jika ", "ketika ", "setiap kali "].iter().any(|p| kecil.starts_with(p)) && kecil.len() > 20 && ["baterai", "flash", "wifi", "wi-fi", "berkas baru", "file baru", "dibuka", "dicolok"].iter().any(|k| kecil.contains(k)) {
        return Some(vec![Call { tool: "pemicu".into(), arg: format!("buat {}", goal.trim()) }]);
    }
    for p in ["jadwalkan", "tolong jadwalkan", "buat jadwal", "buatkan jadwal", "otomatiskan"] {
        if kecil.starts_with(p) && kecil.len() > p.len() + 3 {
            return Some(vec![Call { tool: "jadwalkan".into(), arg: format!("buat {}", goal.trim()[p.len()..].trim()) }]);
        }
    }
    if (kecil.starts_with("setiap ") || kecil.starts_with("tiap ")) && (kecil.contains(" jam ") || kecil.contains(" menit")) {
        return Some(vec![Call { tool: "jadwalkan".into(), arg: format!("buat {}", goal.trim()) }]);
    }
    // Frasa pembantu kerja didahulukan dari makro lama ("rekap hari ini" dulu memicu agenda+tidur+kesehatan).
    for (frasa, arg) in [("mode senyap on", "senyap on"), ("mode senyap off", "senyap off"), ("nova diam", "senyap on"), ("diam dulu nova", "senyap on"), ("tampilkan nova", "senyap off"), ("nova muncul lagi", "senyap off"), ("rekap hari ini", "rekap"), ("rekap kerja", "rekap"), ("rekap harian", "rekap"), ("mulai hari kerja", "mulai"), ("mulai kerja lengkap", "mulai"), ("akhiri kerja", "akhir"), ("selesai kerja", "akhir")] {
        if kecil == frasa {
            return Some(vec![Call { tool: "kerja".into(), arg: arg.into() }]);
        }
    }
    if let Some(m) = crate::aturan::makro(goal) {
        return Some(m);
    }
    let mut segs = vec![goal.to_string()];
    for sep in [" lalu ", " kemudian ", " terus ", " lantas ", " setelah itu ", " dan buat ", " dan buatkan ", " dan tampilkan ", " dan beri ", " dan berikan ", " serta buat ", " habis itu ", " abis itu ", " selanjutnya ", ";", " then "] {
        segs = segs.iter().flat_map(|s| s.split(sep).map(String::from).collect::<Vec<_>>()).collect();
    }
    segs.retain(|s| !s.trim().is_empty());
    let one = segs.len() == 1;
    let mut calls = vec![];
    for s in &segs {
        let s = s.trim();
        match crate::aturan::rule(s) {
            Some(c) => {
                // Belajar online: kalimat ini sekarang menjadi contoh untuk niat tersebut.
                if one {
                    if let Some(l) = label_of(&c.tool) {
                        crate::ml::learn(l, s);
                    }
                }
                calls.push(c);
            }
            // Cadangan ML: klasifikasi niat dari kalimat bebas.
            None if one => {
                if let Some((label, _)) = crate::ml::classify(s) {
                    FROM_ML.store(true, Ordering::Relaxed);
                    calls.push(call_for(&label, s)?);
                } else if weak_tanya(s) {
                    calls.push(Call { tool: "tanya".into(), arg: s.to_string() });
                } else {
                    return None;
                }
            }
            None => return None,
        }
    }
    (!calls.is_empty()).then_some(calls)
}

/// Kalimat bernada pertanyaan yang tidak dikenali aturan maupun ML -> tetap dicari di memori.
fn weak_tanya(s: &str) -> bool {
    let w = s.to_lowercase();
    let first = w.split(|c: char| !c.is_alphanumeric()).find(|x| !x.is_empty()).unwrap_or("");
    s.trim_end().ends_with('?') || ["apa", "bagaimana", "berapa", "kenapa", "mengapa", "jelaskan", "pilih", "resistor", "drop", "cara", "mengapa", "kapan", "siapa"].contains(&first)
}

pub fn label_of(tool: &str) -> Option<&'static str> {
    Some(match tool {
        "tanya" => "tanya",
        "health" => "kesehatan",
        "ps" => "proses",
        "baterai" => "baterai",
        "agenda" => "agenda",
        "habit" => "habit",
        "tidur" => "tidur",
        "layar" => "layar",
        "ngobrol" => "ngobrol",
        "berita" => "berita",
        "kurikulum" => "kurikulum",
        "bukahasil" => "cari",
        "tugas" => "tugas",
        "atur" => "atur",
        "winapp" => "buka",
        "waktu" => "waktu",
        "cli" => "cli",
        "bukafile" => "cari",
        "wifi" | "port" => "cek",
        "jadwal" => "tidur",
        "otomatis" => "atur",
        "bukapath" => "cari",
        "kode" => "ketik",
        "gambar" | "jurnal" | "wiki" | "berpikir" | "terjemah" | "halaman" | "peramban" | "csv" => "baca",
        "gitringkas" => "cek",
        "harga" | "pantau" | "cuacapantau" => "web",
        "hitung" | "konversi" | "ohm" | "fase3" | "hari" | "zona" | "password" => "cek",
        "catat" | "catatan" => "alarm",
        "cuaca" | "kurs" => "web",
        "bersih" => "sapu",
        "web" => "web",
        "musik" => "musik",
        "email" => "email",
        "apps" => "apps",
        "rapikan" => "rapikan",
        "cari" => "cari",
        "meme" => "meme",
        _ => return None,
    })
}

pub fn call_for(label: &str, text: &str) -> Option<Call> {
    let (tool, arg) = match label {
        "tanya" => ("tanya", text),
        "kesehatan" => ("health", ""),
        "proses" => ("ps", ""),
        "baterai" => ("baterai", ""),
        "agenda" => ("agenda", ""),
        "habit" => ("habit", ""),
        "tidur" => ("tidur", ""),
        "layar" => ("layar", ""),
        "ngobrol" => ("ngobrol", text),
        "berita" => ("berita", ""),
        "email" => ("email", ""),
        "tugas" => ("tugas", ""),
        "waktu" => ("waktu", ""),
        "musik" => ("musik", ""),
        "apps" => ("apps", ""),
        "rapikan" => ("rapikan", text),
        "cari" => ("cari", text),
        "meme" => ("meme", text),
        "kurikulum" => ("kurikulum", ""),
        "wifi" => ("wifi", ""),
        "port" => ("port", ""),
        "jadwal" => ("jadwal", ""),
        // label baru dari contoh_niat.json = nama alat di katalog (alat.json); argumennya kalimat itu sendiri
        l if crate::registry::ada(l) => (l, text),
        _ => return None,
    };
    Some(Call { tool: tool.into(), arg: arg.into() })
}

/// Obrolan ringan tanpa model.
fn ngobrol(t: &str) -> String {
    let l = t.to_lowercase();
    let cfg = crate::teknik::teks_json();
    let kata: Vec<&str> = l.split_whitespace().collect();
    // pemicu yang cocok dan PALING PANJANG (paling spesifik) menang; seri → entri yang lebih akhir (tambahan Bos)
    let padded = format!(" {} ", kata.join(" "));
    let mut terbaik: Option<(usize, &serde_json::Value)> = None;
    for r in cfg["ngobrol"].as_array().into_iter().flatten() {
        let mut skor = 0;
        for k in r["pemicu"].as_array().into_iter().flatten().filter_map(|x| x.as_str()) {
            if padded.contains(&format!(" {} ", k.to_lowercase())) {
                skor = skor.max(k.len());
            }
        }
        for k in r["kata"].as_array().into_iter().flatten().filter_map(|x| x.as_str()) {
            if kata.contains(&k) {
                skor = skor.max(k.len());
            }
        }
        if skor > 0 && terbaik.is_none_or(|(s, _)| skor >= s) {
            terbaik = Some((skor, r));
        }
    }
    if let Some((_, r)) = terbaik {
        // beberapa variasi balasan: pilih bergantian berdasarkan detik
        let mut pil: Vec<&str> = r["balasan"].as_str().into_iter().collect();
        pil.extend(r["variasi"].as_array().into_iter().flatten().filter_map(|x| x.as_str()));
        let b = pil.get(chrono::Local::now().timestamp() as usize % pil.len().max(1)).copied().unwrap_or("");
        return b
            .replace("{nama}", crate::agents::NAME)
            .replace("{sapa}", &crate::sistem::salam())
            .replace("{ringkas}", &if b.contains("{ringkas}") { crate::monitor::summary().split(" | ").take(3).collect::<Vec<_>>().join(" · ") } else { String::new() });
    }
    cfg["ngobrol_lainnya"].as_str().unwrap_or("Siap, Bos.").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn c(t: &str, a: &str) -> Call {
        Call { tool: t.into(), arg: a.into() }
    }
    #[test]
    fn youtube_and_fillers() {
        let p = plan_offline("halo nova coba buka youtube dan carikan video anime terbaru").unwrap();
        assert_eq!(p[0].tool, "open");
        assert!(p[0].arg.starts_with("https://www.youtube.com/results?search_query=anime"));
        assert!(matches!(assess(&p[0]), Risk::Safe));
        assert!(matches!(assess(&c("open", "https://x.id/a&b=1")), Risk::Block(_)));
        assert_eq!(plan_offline("kalo aku tidur sekarang maka rencananya jadi seperti apa").unwrap()[0].arg, "sekarang");
        assert_eq!(plan_offline("coba carikan gambar relay di internet").unwrap()[0].arg, "relay");
    }

    /// Penjaga urutan aturan: tiap kalimat harus menghasilkan alat yang diharapkan (aturan baru tak boleh menimpa yang lama).
    #[test]
    fn json_macro_and_ngobrol() {
        let m = plan_offline("mulai kerja").unwrap();
        assert_eq!(m.iter().map(|c| c.tool.as_str()).collect::<Vec<_>>(), ["health", "agenda", "tugas"]);
        assert_eq!(plan_offline("kirim ke email saya laporan | isi").unwrap()[0].tool, "kirim");
        assert!(ngobrol("makasih ya").to_lowercase().contains("sama-sama") || ngobrol("makasih ya").to_lowercase().contains("terima kasih") || ngobrol("makasih ya").len() > 5);
        assert!(ngobrol("siapa kamu").contains("NOVA") || ngobrol("siapa kamu").contains("asisten"));
    }
    #[test]
    fn ordered_steps() {
        let p = plan_offline("buka youtube cari lagu santai setelah itu cek baterai lalu agenda").unwrap();
        let tools: Vec<&str> = p.iter().map(|c| c.tool.as_str()).collect();
        assert_eq!(tools, ["open", "baterai", "agenda"]);
    }
    #[test]
    fn web_services() {
        let url = |t: &str| crate::aturan::rule(t).map(|c| (c.tool, c.arg));
        let (tool, arg) = url("carikan di video youtube anime fullmetal").unwrap();
        assert_eq!(tool, "open");
        assert!(arg.starts_with("https://www.youtube.com/results?search_query=anime%20fullmetal"));
        assert!(url("kamu ganti music ke lagu indonesia").unwrap().1.contains("search_query=lagu%20indonesia"));
        assert!(url("tolong putarkan lagu dangdut koplo").unwrap().1.contains("lagu%20dangdut%20koplo"));
        // data JSON: menambah layanan dan sinonim aksi tanpa mengubah kode
        assert!(crate::aturan::layanan_tambah("wikipedia wiki https://id.wikipedia.org/w/index.php?search={q}").contains("ditambahkan"));
        assert!(url("cari motor induksi di wiki").unwrap().1.contains("search=motor%20induksi"));
        assert!(!crate::kata::punya_aksi("dengerin lagu", "putar"));
        crate::kata::tambah_aksi("putar", &["dengerin"]);
        assert!(crate::kata::punya_aksi("dengerin lagu", "putar"));
        for t in ["buka music anime", "buka lagu anime", "putar lagu anime", "nyalakan musik anime"] {
            assert!(url(t).unwrap().1.contains("search_query=lagu%20anime"), "{t}");
        }
        assert!(!url("buka spotify music").is_some_and(|c| c.1.contains("youtube")));
        assert!(url("rute ke bandung").unwrap().1.contains("google.com/maps/search/bandung"));
        assert!(url("carikan repo rust pet di github").unwrap().1.contains("github.com/search"));
        assert!(url("buka video anime fullmetal").unwrap().1.contains("youtube.com/results?search_query=anime%20fullmetal"));
        assert_ne!(url("buka video.mp4").map(|c| c.1.contains("youtube")), Some(true));
        assert_ne!(url("carikan file video mp4 kemarin").map(|c| c.0), Some("open".into()));
    }
    #[test]
    fn rules_golden() {
        let cases = [
            ("hitung 2+3*4", "hitung"),
            ("cuaca di bandung", "cuaca"),
            ("harga bitcoin", "harga"),
            ("catat beli kabel besok", "catat"),
            ("jam 14:00 wib di london", "zona"),
            ("ohm V=12 R=4", "ohm"),
            ("berapa hari lagi ke 17 agustus", "hari"),
            ("pantau bbca di bawah 1000", "pantau"),
            ("ingatkan kalau hujan di bandung", "cuacapantau"),
            ("kalo aku tidur sekarang maka rencananya jadi seperti apa", "jadwal"),
            ("bisa tidur 8 jam malam ini", "jadwal"),
            ("coba carikan gambar relay di internet", "gambar"),
            ("carikan jurnal semantic 2021 sampai 2025", "jurnal"),
            ("buka vs code folder digital nameplate", "kode"),
            ("bersihkan cache aman", "bersih"),
            ("halo nova coba buka youtube dan carikan video anime terbaru", "open"),
            ("analisis csv C:/a.csv", "csv"),
            ("wikipedia transformator", "wiki"),
            ("konversi 5 km ke mil", "konversi"),
            ("cek port yang aktif", "port"),
            ("cek wifi disini apa aja", "wifi"),
            ("buka kalkulator", "winapp"),
        ];
        for (q, tool) in cases {
            let p = plan_offline(q).unwrap_or_default();
            assert_eq!(p.first().map(|c| c.tool.as_str()), Some(tool), "kalimat: {q}");
        }
    }
    #[test]
    fn risk_levels() {
        assert!(matches!(assess(&c("info", "")), Risk::Safe));
        assert!(matches!(assess(&c("shell", "dir")), Risk::Safe));
        assert!(matches!(assess(&c("shell", "mkdir x")), Risk::Confirm(_)));
        assert!(matches!(assess(&c("shell", "type C:/u/.ssh/id_rsa")), Risk::Confirm(_)));
        assert!(matches!(assess(&c("shell", "dir > a.txt")), Risk::Confirm(_)));
        assert!(matches!(assess(&c("shell", "rm -rf /")), Risk::Block(_)));
        assert!(matches!(assess(&c("shell", "echo x | powershell -")), Risk::Confirm(_)));
        assert!(matches!(assess(&c("shell", "dir nonexist || del a.txt")), Risk::Confirm(_)));
        assert!(matches!(assess(&c("shell", "git branch -D kerja")), Risk::Confirm(_)));
        assert!(matches!(assess(&c("shell", "git diff --output=x.txt")), Risk::Confirm(_)));
        assert!(matches!(assess(&c("shell", "git status")), Risk::Safe));
        assert!(matches!(assess(&c("shell", "dir \"C:\\Program Files\"")), Risk::Safe));
        assert!(matches!(assess(&c("open", "C:/Users/me/Downloads/setup.exe")), Risk::Confirm(_)));
        assert!(matches!(assess(&c("open", "C:/Users/me/Documents/laporan.pdf")), Risk::Safe));
        assert!(matches!(assess(&c("open", "calc")), Risk::Safe));
        for ip in ["127.0.0.1", "10.0.0.5", "192.168.1.1", "172.20.0.1", "169.254.169.254", "::1", "fd00::1", "0.0.0.0"] {
            assert!(!publik(&ip.parse().unwrap()), "{ip}");
        }
        assert!(publik(&"8.8.8.8".parse().unwrap()));
        assert!(matches!(assess(&c("read", "C:/u/.ssh/id_rsa")), Risk::Confirm(_)));
        assert!(matches!(assess(&c("open", "a&calc")), Risk::Block(_)));
        assert!(matches!(assess(&c("xx", "")), Risk::Block(_)));
    }
    #[test]
    fn offline_rules() {
        let p = plan_offline("isi folder C:/Users/me/Downloads lalu cek kesehatan laptop").unwrap();
        assert_eq!((p[0].tool.as_str(), p[0].arg.as_str()), ("ls", "C:/Users/me/Downloads"));
        assert_eq!(p[1].tool, "health");
        assert_eq!(plan_offline("buka https://rust-lang.org").unwrap()[0].tool, "open");
        assert_eq!(plan_offline("jalankan ipconfig").unwrap()[0].arg, "ipconfig");
        assert!(plan_offline("tolong buatkan puisi").is_none());
    }
    #[test]
    fn summary_picks_sentences() {
        let t = "Rust adalah bahasa pemrograman sistem yang cepat dan aman. Rust memakai ownership untuk keamanan memori. \
Cuaca hari ini cerah sekali di kota bandung dan sekitarnya. Ownership di Rust mencegah data race pada program konkuren.";
        let s = summarize(t, 2);
        assert!(s.contains("Rust"));
        assert_eq!(plan_offline("ringkas pdf C:/a/b.pdf").unwrap()[0].tool, "ringkas");
    }
    #[test]
    fn json_extraction() {
        assert!(parse_json("ok {\"a\":1} bye").is_some());
        assert!(parse_json("tanpa json").is_none());
    }

    #[tokio::test]
    async fn alat_web_menolak_alamat_lokal() {
        for u in ["http://127.0.0.1:47831/ui", "http://localhost/ui", "http://[::1]/", "http://192.168.1.1/", "http://169.254.169.254/latest", "file:///C:/x"] {
            assert!(url_publik(u).await.is_err(), "{u}");
        }
        assert!(url_publik("http://8.8.8.8/").await.is_ok());
    }
}
