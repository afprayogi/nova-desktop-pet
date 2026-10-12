//! Bantuan perintah CLI: `bantu git commit`, `cmd? cargo` -> menjalankan HANYA `--help`/`-h` alat yang diizinkan dan menampilkan
//! ringkasnya. Juga daftar perintah obrolan NOVA (`SLASH`) untuk penyelesaian otomatis (Tab) dan pencarian (`/bantuan <kata>`).
use std::process::{Command, Stdio};
use std::time::Duration;

/// Daftar alat CLI yang boleh dibaca bantuannya (dibuat ke `cli.json`; tambah lewat berkas atau `/cli tambah`). Hanya flag bantuan yang dijalankan.
pub const CLI_BAWAAN: &str = r#"[
  {"nama": "git", "bantuan": "-h", "deskripsi": "kontrol versi kode"},
  {"nama": "cargo", "bantuan": "--help", "deskripsi": "build dan paket Rust"},
  {"nama": "rustc", "bantuan": "--help", "deskripsi": "kompiler Rust"},
  {"nama": "rustup", "bantuan": "--help", "deskripsi": "pengelola toolchain Rust"},
  {"nama": "docker", "bantuan": "--help", "deskripsi": "kontainer"},
  {"nama": "ollama", "bantuan": "--help", "deskripsi": "model bahasa lokal"},
  {"nama": "npm", "bantuan": "--help", "deskripsi": "paket Node.js"},
  {"nama": "node", "bantuan": "--help", "deskripsi": "JavaScript runtime"},
  {"nama": "python", "bantuan": "--help", "deskripsi": "interpreter Python"},
  {"nama": "pip", "bantuan": "--help", "deskripsi": "paket Python"},
  {"nama": "winget", "bantuan": "--help", "deskripsi": "pemasang aplikasi Windows"},
  {"nama": "gh", "bantuan": "--help", "deskripsi": "GitHub CLI"},
  {"nama": "ping", "bantuan": "/?", "deskripsi": "uji koneksi"},
  {"nama": "ipconfig", "bantuan": "/?", "deskripsi": "konfigurasi IP"},
  {"nama": "tasklist", "bantuan": "/?", "deskripsi": "daftar proses"},
  {"nama": "netstat", "bantuan": "/?", "deskripsi": "koneksi dan port"},
  {"nama": "robocopy", "bantuan": "/?", "deskripsi": "salin folder tangguh"},
  {"nama": "dir", "bantuan": "/?", "deskripsi": "isi folder"},
  {"nama": "where", "bantuan": "/?", "deskripsi": "cari lokasi perintah"},
  {"nama": "powershell", "bantuan": "-?", "deskripsi": "shell PowerShell"},
  {"nama": "ffmpeg", "bantuan": "--help", "deskripsi": "konversi audio/video"},
  {"nama": "curl", "bantuan": "--help", "deskripsi": "permintaan HTTP"},
  {"nama": "tailscale", "bantuan": "--help", "deskripsi": "jaringan pribadi"},
  {"nama": "kicad-cli", "bantuan": "--help", "deskripsi": "KiCad (PCB/skematik) lewat CLI"},
  {"nama": "code", "bantuan": "--help", "deskripsi": "VS Code"},
  {"nama": "magick", "bantuan": "--help", "deskripsi": "ImageMagick (olah gambar)"},
  {"nama": "yt-dlp", "bantuan": "--help", "deskripsi": "unduh video"},
  {"nama": "pnpm", "bantuan": "--help", "deskripsi": "paket Node.js"},
  {"nama": "yarn", "bantuan": "--help", "deskripsi": "paket Node.js"},
  {"nama": "uv", "bantuan": "--help", "deskripsi": "paket Python cepat"},
  {"nama": "go", "bantuan": "--help", "deskripsi": "bahasa Go"},
  {"nama": "dotnet", "bantuan": "--help", "deskripsi": ".NET"},
  {"nama": "adb", "bantuan": "--help", "deskripsi": "Android Debug Bridge"}
]"#;

const FLAG_AMAN: &[&str] = &["--help", "-h", "/?", "-?", "help", "--version", "-help"];

pub struct Alat {
    pub nama: String,
    pub bantuan: String,
    pub deskripsi: String,
    /// Boleh dijalankan `<nama> <bantuan>`: hanya alat bawaan terverifikasi, atau entri dengan `"aman": true` di cli.json.
    pub aman: bool,
}

fn bawaan_aman() -> Vec<String> {
    serde_json::from_str::<Vec<serde_json::Value>>(CLI_BAWAAN).unwrap_or_default().iter().filter_map(|x| x["nama"].as_str().map(String::from)).collect()
}

pub fn daftar() -> Vec<Alat> {
    crate::data::json("cli.json", CLI_BAWAAN)
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    let nama = clean_word(x["nama"].as_str()?)?;
                    let f = x["bantuan"].as_str().unwrap_or("--help");
                    let aman = x["aman"].as_bool().unwrap_or_else(|| bawaan_aman().contains(&nama));
                    Some(Alat { nama, bantuan: if FLAG_AMAN.contains(&f) { f.to_string() } else { "--help".into() }, deskripsi: x["deskripsi"].as_str().unwrap_or("").to_string(), aman })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn nama_alat() -> Vec<String> {
    daftar().into_iter().map(|a| a.nama).collect()
}

/// `/cli tambah <nama> [flag-bantuan] [| deskripsi]` → cli.json.
pub fn tambah(arg: &str) -> String {
    let (kiri, desk) = arg.split_once('|').map(|(a, b)| (a.trim(), b.trim())).unwrap_or((arg.trim(), ""));
    let mut w = kiri.split_whitespace();
    let Some(nama) = w.next().and_then(clean_word) else { return "Pakai: /cli tambah <nama> [--help|-h|/?] | <deskripsi>".into() };
    let flag = w.next().filter(|f| FLAG_AMAN.contains(f)).unwrap_or("--help").to_string();
    let (n2, f2, d2) = (nama.clone(), flag.clone(), desk.to_string());
    match crate::data::ubah("cli.json", CLI_BAWAAN, move |v| {
        if let Some(a) = v.as_array_mut() {
            a.retain(|x| x["nama"].as_str() != Some(n2.as_str()));
            a.push(serde_json::json!({"nama": n2, "bantuan": f2, "deskripsi": d2}));
        }
    }) {
        Ok(()) => format!("Alat CLI '{nama}' ditambahkan (bantuan: {flag}). '/cli belajar' memasukkan cara pakainya ke pengetahuan."),
        Err(e) => format!("Gagal menyimpan cli.json: {e}"),
    }
}

/// (perintah, deskripsi) obrolan NOVA.
pub const SLASH: &[(&str, &str)] = &[
    ("/bantuan", "daftar perintah; /bantuan <kata> untuk mencari"),
    ("/llm", "Llama lokal ringan: on | off | pasang <model> | model <nama>"),
    ("/gemini", "Gemini Flash: key <kunci> | on | off | model | tes"),
    ("/cerewet", "tingkat bicara NOVA 0|1|2"),
    ("/iklan", "pemblokir iklan: on | off | update | ekstensi"),
    ("/indeks", "indeks berkas semua drive: bangun"),
    ("/graf", "peta graf ingatan: lihat"),
    ("/graphify", "petakan folder kode/dokumen ke graf"),
    ("/email", "email semua akun"),
    ("/google", "akun Google"),
    ("/outlook", "akun Outlook/Microsoft 365"),
    ("/bec", "tes BEC harian: on | off | sekarang"),
    ("/ocr", "baca layar: on | off"),
    ("/minat", "topik berita sesuai minat"),
    ("/proyek", "pantau proyek/git"),
    ("/log", "baca log error"),
    ("/makro", "makro cepat: commit, dll."),
    ("/port", "lihat port yang terpakai"),
    ("/killport", "tutup proses di sebuah port"),
    ("/meme", "meme"),
    ("/aksi", "demo animasi aksi: demo"),
    ("/scan", "pindai model OmniRoute"),
    ("/ollama", "sambungkan Ollama lokal"),
    ("/urungkan", "kembalikan file hasil rapikan terakhir"),
    ("/suka", "tandai tindakan NOVA tadi cocok"),
    ("/tidak", "tandai tindakan NOVA tadi mengganggu"),
    ("/diam", "NOVA diam sebentar"),
    ("/pasang", "pasang model Ollama"),
];

/// Alat di cli.json yang benar-benar ada di PATH laptop ini (PATH dipindai sekali; tidak menjalankan proses apa pun).
pub fn terpasang() -> Vec<String> {
    let mut ada = std::collections::HashSet::new();
    for dir in std::env::var("PATH").unwrap_or_default().split(';').filter(|d| !d.is_empty()) {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().and_then(|x| x.to_str()).is_some_and(|x| ["exe", "cmd", "bat", "com"].contains(&x.to_lowercase().as_str())) {
                    if let Some(st) = p.file_stem().and_then(|x| x.to_str()) {
                        ada.insert(st.to_lowercase());
                    }
                }
            }
        }
    }
    // perintah bawaan cmd (dir) selalu ada
    ada.insert("dir".into());
    nama_alat().into_iter().filter(|n| ada.contains(n)).collect()
}

/// Masukkan alat CLI terpasang ke pengetahuan. Hanya alat bertanda aman yang dijalankan (`--help`, paralel, maks 40);
/// sisanya dicatat dari deskripsi di cli.json tanpa menjalankan apa pun.
pub fn belajar_semua() -> String {
    let ada = terpasang();
    let semua = daftar();
    let (aman, lain): (Vec<&String>, Vec<&String>) = ada.iter().partition(|n| semua.iter().any(|a| &a.nama == *n && a.aman));
    let hs: Vec<_> = aman.iter().take(40).map(|t| { let t = t.to_string(); std::thread::spawn(move || (t.clone(), help(&t))) }).collect();
    let (mut ok, mut n) = (0, 0);
    for h in hs {
        if let Ok((t, teks)) = h.join() {
            if teks.starts_with("📟") && teks.len() > 60 {
                n += crate::teknik::ingest(&format!("cli:{t}"), &format!("Alat CLI {t} (terpasang di laptop ini). {teks}"));
                ok += 1;
            }
        }
    }
    let catatan: String = lain.iter().filter_map(|t| semua.iter().find(|a| &a.nama == *t)).filter(|a| !a.deskripsi.is_empty()).map(|a| format!("{}: {}", a.nama, a.deskripsi)).collect::<Vec<_>>().join("; ");
    if !catatan.is_empty() {
        n += crate::teknik::ingest("cli:daftar", &format!("Alat CLI terpasang di laptop ini (hanya deskripsi, tidak dijalankan): {catatan}"));
    }
    format!("{} alat CLI terpasang; {ok} dibaca bantuannya (aman), {} hanya dicatat deskripsinya; {n} potongan masuk pengetahuan.", ada.len(), lain.len())
}

pub fn is_tool(w: &str) -> bool {
    nama_alat().contains(&w.to_lowercase())
}

pub fn complete(prefix: &str) -> Vec<&'static str> {
    if !prefix.starts_with('/') || prefix.contains(' ') {
        return vec![];
    }
    SLASH.iter().map(|(c, _)| *c).filter(|c| c.starts_with(prefix) && *c != prefix).collect()
}

pub fn find(q: &str) -> String {
    let q = q.trim().to_lowercase();
    let hits: Vec<String> = SLASH.iter().filter(|(c, d)| q.is_empty() || c.contains(&q) || d.to_lowercase().contains(&q)).map(|(c, d)| format!("{c} — {d}")).collect();
    if hits.is_empty() {
        format!("Tidak ada perintah NOVA yang cocok dengan '{q}'. Ketik /bantuan untuk semua, atau tanya bebas (mis. 'kecerahan 40', 'jam berapa', 'cari file pdf motor').")
    } else {
        format!("Perintah NOVA{}:\n{}\nTekan Tab untuk melengkapi, ↑ untuk riwayat. Alat CLI: 'bantu git commit', 'bantu cargo'.", if q.is_empty() { String::new() } else { format!(" untuk '{q}'") }, hits.join("\n"))
    }
}

fn clean_word(w: &str) -> Option<String> {
    (!w.is_empty() && w.len() <= 24 && w.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')).then(|| w.to_lowercase())
}

/// `arg` = "<alat> [subperintah]". Hanya --help/-h; maksimal 2 kata; alat harus ada di daftar putih.
pub fn help(arg: &str) -> String {
    let words: Vec<String> = arg.split_whitespace().filter_map(clean_word).take(2).collect();
    let Some(tool) = words.first() else { return format!("Sebut nama alatnya, mis. 'bantu git commit'. Yang didukung: {}.", nama_alat().join(", ")) };
    let Some(entri) = daftar().into_iter().find(|a| a.nama == *tool) else {
        return format!("'{tool}' tidak ada di daftar alat CLI yang kuizinkan ({}). Tambahkan lewat /cli tambah atau cli.json.", nama_alat().join(", "));
    };
    if !entri.aman {
        return format!("📟 {}: {}. (Bantuannya tidak dijalankan otomatis karena belum ditandai aman; tambahkan \"aman\": true pada entri ini di cli.json bila yakin alat ini menampilkan bantuan untuk {}.)", entri.nama, if entri.deskripsi.is_empty() { "tanpa deskripsi" } else { &entri.deskripsi }, entri.bantuan);
    }
    let flag = entri.bantuan.as_str();
    let mut c = Command::new(if tool == "dir" { "cmd" } else { tool.as_str() });
    if tool == "dir" {
        c.args(["/C", "dir", "/?"]);
    } else {
        c.args(words[1..].iter().map(|s| s.as_str())).arg(flag);
    }
    c.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).env("GIT_PAGER", "cat").env("PAGER", "cat");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let Ok(child) = c.spawn() else { return format!("'{tool}' tidak terpasang di laptop ini (atau tidak ada di PATH).") };
    let pid = child.id();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    let Ok(Ok(o)) = rx.recv_timeout(Duration::from_secs(8)) else {
        // jangan biarkan proses yang tidak merespons tetap hidup
        let mut k = Command::new("taskkill");
        k.args(["/F", "/T", "/PID", &pid.to_string()]).stdout(Stdio::null()).stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            k.creation_flags(0x0800_0000);
        }
        let _ = k.status();
        return format!("'{tool}' tidak merespons (batas 8 detik); proses dihentikan.");
    };
    let text = String::from_utf8_lossy(if o.stdout.is_empty() { &o.stderr } else { &o.stdout }).to_string();
    let lines: Vec<&str> = text.lines().map(|l| l.trim_end()).filter(|l| !l.trim().is_empty()).take(28).collect();
    if lines.is_empty() {
        return format!("'{arg}' tidak menghasilkan bantuan.");
    }
    format!("📟 {} {}:\n{}", words.join(" "), flag, lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn completion_and_whitelist() {
        assert!(complete("/ll").contains(&"/llm"));
        assert!(complete("/llm").is_empty());
        assert!(help("rm -rf").contains("tidak ada di daftar"));
        assert!(find("gemini").contains("/gemini"));
        assert!(clean_word("a;b").is_none());
    }
}
