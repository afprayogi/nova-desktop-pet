//! Bantuan perintah CLI: `bantu git commit`, `cmd? cargo` -> menjalankan HANYA `--help`/`-h` alat yang diizinkan dan menampilkan
//! ringkasnya. Juga daftar perintah obrolan NOVA (`SLASH`) untuk penyelesaian otomatis (Tab) dan pencarian (`/bantuan <kata>`).
use std::process::{Command, Stdio};
use std::time::Duration;

const TOOLS: &[&str] = &["git", "cargo", "rustc", "rustup", "docker", "ollama", "npm", "node", "python", "pip", "winget", "gh", "ping", "ipconfig", "tasklist", "netstat", "robocopy", "dir", "where", "powershell", "ffmpeg", "curl", "tailscale", "kicad-cli", "code", "magick", "yt-dlp", "pnpm", "yarn", "uv", "go", "dotnet", "adb"];

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

/// Alat CLI di daftar putih yang benar-benar ada di PATH laptop ini.
pub fn terpasang() -> Vec<&'static str> {
    TOOLS
        .iter()
        .copied()
        .filter(|t| {
            let mut c = Command::new("where");
            c.arg(t).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                c.creation_flags(0x0800_0000);
            }
            c.status().map(|s| s.success()).unwrap_or(false)
        })
        .collect()
}

/// Baca `--help` semua alat CLI terpasang (paralel) dan simpan ke pengetahuan agar NOVA tahu cara memakainya.
pub fn belajar_semua() -> String {
    let ada = terpasang();
    let hs: Vec<_> = ada.iter().map(|t| { let t = t.to_string(); std::thread::spawn(move || (t.clone(), help(&t))) }).collect();
    let (mut ok, mut n) = (0, 0);
    for h in hs {
        if let Ok((t, teks)) = h.join() {
            if teks.starts_with("📟") && teks.len() > 60 {
                n += crate::teknik::ingest(&format!("cli:{t}"), &format!("Alat CLI {t} (terpasang di laptop ini). {teks}"));
                ok += 1;
            }
        }
    }
    format!("{} alat CLI terpasang ({}); {ok} dipelajari ke pengetahuan ({n} potongan). Tanya mis. 'cara pakai git stash'.", ada.len(), ada.join(", "))
}

pub fn is_tool(w: &str) -> bool {
    TOOLS.contains(&w.to_lowercase().as_str())
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
    let Some(tool) = words.first() else { return format!("Sebut nama alatnya, mis. 'bantu git commit'. Yang didukung: {}.", TOOLS.join(", ")) };
    if !TOOLS.contains(&tool.as_str()) {
        return format!("'{tool}' tidak ada di daftar alat CLI yang kuizinkan ({}).", TOOLS.join(", "));
    }
    let flag = if tool == "git" || tool == "dir" { "-h" } else if tool == "ping" || tool == "ipconfig" || tool == "tasklist" || tool == "netstat" || tool == "robocopy" || tool == "where" { "/?" } else { "--help" };
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
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    let Ok(Ok(o)) = rx.recv_timeout(Duration::from_secs(8)) else { return format!("'{tool}' tidak merespons (batas 8 detik).") };
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
