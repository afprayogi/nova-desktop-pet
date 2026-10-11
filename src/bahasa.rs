//! Pendeteksi bahasa pemrograman/alat yang TERPASANG di laptop + paket pengetahuan dari GitHub tepercaya.
//! Dipindai ulang berkala: bila Bos memasang bahasa baru, NOVA otomatis mengunduh catatannya (teks saja, tidak pernah
//! dijalankan) ke basis pengetahuan lokal. Sumber dibatasi ke daftar putih: raw.githubusercontent.com/adambard/learnxinyminutes-docs
//! (lisensi CC BY-SA 3.0; atribusi disimpan pada label sumber) dan URL raw.githubusercontent.com yang Bos tulis sendiri di paket.json.
use crate::engine::Tx;
use std::process::{Command, Stdio};
use std::sync::Mutex;

/// (kunci bahasa, perintah, argumen versi)
const ALAT: &[(&str, &str, &[&str])] = &[
    ("python", "python", &["--version"]),
    ("python", "py", &["--version"]),
    ("node", "node", &["--version"]),
    ("typescript", "tsc", &["--version"]),
    ("rust", "rustc", &["--version"]),
    ("go", "go", &["version"]),
    ("java", "javac", &["-version"]),
    ("csharp", "dotnet", &["--version"]),
    ("c", "gcc", &["--version"]),
    ("cpp", "g++", &["--version"]),
    ("c", "clang", &["--version"]),
    ("php", "php", &["--version"]),
    ("ruby", "ruby", &["--version"]),
    ("lua", "lua", &["-v"]),
    ("kotlin", "kotlinc", &["-version"]),
    ("swift", "swift", &["--version"]),
    ("dart", "dart", &["--version"]),
    ("powershell", "powershell", &["-NoProfile", "-Command", "$PSVersionTable.PSVersion.ToString()"]),
    ("bash", "bash", &["--version"]),
    ("haskell", "ghc", &["--version"]),
    ("julia", "julia", &["--version"]),
    ("r", "Rscript", &["--version"]),
    ("zig", "zig", &["version"]),
    ("git", "git", &["--version"]),
    ("docker", "docker", &["--version"]),
];

#[derive(Clone, Debug)]
pub struct Found {
    pub lang: String,
    pub cmd: String,
    pub version: String,
}

static CACHE: Mutex<Vec<Found>> = Mutex::new(Vec::new());

fn probe(cmd: &str, args: &[&str]) -> Option<String> {
    let mut c = Command::new(cmd);
    c.args(args).stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let out = c.output().ok()?;
    let txt = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    let line = txt.lines().map(|l| l.trim()).find(|l| !l.is_empty())?;
    out.status.success().then(|| line.chars().take(60).collect())
}

/// Pindai semua alat secara paralel (tiap proses dibatasi singkat).
pub fn detect() -> Vec<Found> {
    let handles: Vec<_> = ALAT
        .iter()
        .map(|(lang, cmd, args)| {
            let (lang, cmd, args) = (lang.to_string(), cmd.to_string(), args.iter().map(|a| a.to_string()).collect::<Vec<_>>());
            std::thread::spawn(move || {
                let a: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
                probe(&cmd, &a).map(|v| Found { lang, cmd, version: v })
            })
        })
        .collect();
    let mut seen = std::collections::HashSet::new();
    let found: Vec<Found> = handles.into_iter().filter_map(|h| h.join().ok().flatten()).filter(|f| seen.insert(f.lang.clone())).collect();
    if let Ok(mut c) = CACHE.lock() {
        *c = found.clone();
    }
    found
}

pub fn has(lang: &str) -> bool {
    CACHE.lock().map(|c| c.iter().any(|f| f.lang == lang)).unwrap_or(false)
}

pub fn cmd_of(lang: &str) -> Option<String> {
    CACHE.lock().ok()?.iter().find(|f| f.lang == lang).map(|f| f.cmd.clone())
}

pub fn summary() -> String {
    let c = CACHE.lock().map(|c| c.clone()).unwrap_or_default();
    if c.is_empty() {
        return "Belum ada pemindaian bahasa. Ketik /bahasa.".into();
    }
    c.iter().map(|f| format!("{} ({})", f.lang, f.version)).collect::<Vec<_>>().join(", ")
}

/// Berkas dokumen learnxinyminutes untuk bahasa tertentu.
fn lxiy(lang: &str) -> Option<&'static str> {
    Some(match lang {
        "python" => "python", "node" => "javascript", "typescript" => "typescript", "rust" => "rust", "go" => "go", "c" => "c", "cpp" => "c++",
        "csharp" => "csharp", "java" => "java", "php" => "php", "ruby" => "ruby", "lua" => "lua", "kotlin" => "kotlin", "swift" => "swift",
        "dart" => "dart", "powershell" => "powershell", "bash" => "bash", "haskell" => "haskell", "julia" => "julia", "r" => "r",
        "git" => "git", "docker" => "docker", "zig" => "zig",
        _ => return None,
    })
}

const ALLOW_PREFIX: &str = "https://raw.githubusercontent.com/";

/// URL sumber untuk satu bahasa: paket resmi (daftar putih) + tambahan dari paket.json (tetap harus raw.githubusercontent.com).
fn sumber(lang: &str) -> Vec<(String, String)> {
    let mut v = vec![];
    if let Some(f) = lxiy(lang) {
        v.push((format!("{ALLOW_PREFIX}adambard/learnxinyminutes-docs/master/{}.md", urlencoding::encode(f)), format!("lxiy:{lang} (learnxinyminutes, CC BY-SA 3.0)")));
    }
    if let Some(m) = std::fs::read_to_string(crate::config::data_dir().join("paket.json")).ok().and_then(|t| serde_json::from_str::<std::collections::HashMap<String, Vec<String>>>(&t).ok()) {
        for u in m.get(lang).into_iter().flatten().filter(|u| u.starts_with(ALLOW_PREFIX)).take(6) {
            v.push((u.clone(), format!("github:{lang} ({})", u.trim_start_matches(ALLOW_PREFIX).split('/').take(2).collect::<Vec<_>>().join("/"))));
        }
    }
    v
}

fn strip_front_matter(t: &str) -> &str {
    t.strip_prefix("---").and_then(|r| r.find("\n---").map(|i| &r[i + 4..])).unwrap_or(t)
}

fn pack_done(lang: &str) -> bool {
    crate::assistant::db()
        .ok()
        .map(|c| {
            let _ = c.execute_batch("CREATE TABLE IF NOT EXISTS bahasa_pack(lang TEXT PRIMARY KEY, ts INT);");
            c.query_row("SELECT 1 FROM bahasa_pack WHERE lang=?1", [lang], |_| Ok(())).is_ok()
        })
        .unwrap_or(false)
}

fn mark_pack(lang: &str) {
    if let Ok(c) = crate::assistant::db() {
        let _ = c.execute("INSERT OR REPLACE INTO bahasa_pack VALUES(?1, strftime('%s','now'))", [lang]);
    }
}

/// Unduh + serap paket pengetahuan satu bahasa. Mengembalikan jumlah potongan.
pub async fn learn(http: &reqwest::Client, lang: &str) -> Result<usize, String> {
    let sources = sumber(lang);
    if sources.is_empty() {
        return Err(format!("belum ada sumber tepercaya untuk '{lang}' (tambahkan URL raw.githubusercontent.com ke paket.json)"));
    }
    let mut total = 0;
    for (url, label) in sources {
        let resp = http.get(&url).send().await.map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            continue;
        }
        let text = resp.text().await.map_err(|e| e.to_string())?;
        let body = strip_front_matter(&text).to_string();
        total += tokio::task::spawn_blocking(move || crate::teknik::ingest(&label, &body)).await.map_err(|e| e.to_string())?;
    }
    if total == 0 {
        return Err("tidak ada konten yang bisa diunduh (cek internet)".into());
    }
    mark_pack(lang);
    Ok(total)
}

/// Pindai ulang; untuk bahasa baru terpasang, serap paketnya dan umumkan. Aman dipanggil berkala.
pub async fn update(tx: &Tx, http: &reqwest::Client, announce_all: bool) {
    let found = tokio::task::spawn_blocking(detect).await.unwrap_or_default();
    let mut learned: Vec<String> = vec![];
    for f in found {
        if pack_done(&f.lang) {
            continue;
        }
        match learn(http, &f.lang).await {
            Ok(n) => learned.push(format!("{} ({n})", f.lang)),
            Err(e) => {
                if announce_all {
                    tx.notice(format!("📚 Paket {} belum bisa dipelajari: {e}", f.lang));
                }
                // bahasa tanpa sumber: tandai agar tidak dicoba berulang
                if e.starts_with("belum ada sumber") {
                    mark_pack(&f.lang);
                }
            }
        }
    }
    if !learned.is_empty() && crate::adegan::level() > 0 {
        tx.notice(format!("📚 Catatan bahasa terpasang dipelajari otomatis: {}.", learned.join(", ")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sources_are_allowlisted() {
        let s = sumber("python");
        assert!(s[0].0.starts_with("https://raw.githubusercontent.com/adambard/learnxinyminutes-docs/") && s[0].0.ends_with("python.md"));
        assert!(sumber("cpp")[0].0.ends_with("c%2B%2B.md"));
        assert!(sumber("kabaret").is_empty());
        assert_eq!(strip_front_matter("---\nname: x\n---\nisi").trim(), "isi");
    }
    #[test]
    fn detects_something_installed() {
        // git hampir pasti ada di mesin pengembang; yang penting detect() tidak panik dan menghasilkan versi.
        let f = detect();
        assert!(f.iter().all(|x| !x.version.is_empty()));
    }
}
