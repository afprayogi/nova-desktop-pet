//! Makro & alat terminal deterministik: (1) makro proyek (build/test/git commit & push, plus makro buatan Bos di makro.json),
//! (2) pembersih port nyangkut (/port, /killport), (3) pembuat kerangka proyek dari template murni ({{Name}} dst.).
//! Semua dijalankan HANYA atas perintah/klik Bos (bukan oleh model); proses sistem penting dilindungi.
use crate::config::data_dir;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn no_window(c: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    #[cfg(not(windows))]
    let _ = c;
}

// ---------- Makro ----------

/// (nama, perintah) dari jenis proyek aktif + makro.json (peta nama->perintah).
pub fn macros_for(dir: &Path) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = vec![];
    let has = |f: &str| dir.join(f).exists();
    if has("Cargo.toml") {
        v.extend([("build", "cargo build"), ("test", "cargo test"), ("cek", "cargo check")].map(|(a, b)| (a.to_string(), b.to_string())));
    }
    if has("package.json") {
        v.extend([("build", "npm run build"), ("test", "npm test")].map(|(a, b)| (a.to_string(), b.to_string())));
    }
    if has("pyproject.toml") || has("requirements.txt") || has("pytest.ini") {
        v.push(("test".into(), "python -m pytest -q".into()));
    }
    if has("go.mod") {
        v.extend([("build", "go build ./..."), ("test", "go test ./...")].map(|(a, b)| (a.to_string(), b.to_string())));
    }
    if std::fs::read_dir(dir).map(|rd| rd.flatten().any(|e| e.path().extension().is_some_and(|x| x == "sln" || x == "csproj"))).unwrap_or(false) {
        v.extend([("build", "dotnet build"), ("test", "dotnet test")].map(|(a, b)| (a.to_string(), b.to_string())));
    }
    if dir.join(".git").exists() {
        v.push(("git".into(), "git status -sb".into()));
        v.push(("diff".into(), "git diff --stat".into()));
    }
    if let Some(m) = std::fs::read_to_string(data_dir().join("makro.json")).ok().and_then(|t| serde_json::from_str::<HashMap<String, String>>(&t).ok()) {
        v.extend(m);
    }
    let mut seen = std::collections::HashSet::new();
    v.retain(|(n, _)| seen.insert(n.clone()));
    v
}

/// Nama makro (untuk tombol di peliharaan).
pub fn names() -> Vec<String> {
    let mut n: Vec<String> = crate::proyek::active().map(|d| macros_for(&d).into_iter().map(|x| x.0).collect()).unwrap_or_default();
    if n.iter().any(|x| x == "git") {
        n.push("commit".into());
    }
    n
}

async fn exec(cmdline: &str, dir: &Path, secs: u64) -> String {
    let mut c = tokio::process::Command::new("cmd");
    c.args(["/C", cmdline]).current_dir(dir).kill_on_drop(true).stdin(std::process::Stdio::null());
    #[cfg(windows)]
    {
        c.creation_flags(0x0800_0000);
    }
    match tokio::time::timeout(Duration::from_secs(secs), c.output()).await {
        Err(_) => format!("⏱ dihentikan: lebih dari {secs} detik"),
        Ok(Err(e)) => format!("gagal menjalankan: {e}"),
        Ok(Ok(o)) => {
            let t = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
            let t = t.trim();
            let tail: String = t.chars().rev().take(1500).collect::<Vec<_>>().into_iter().rev().collect();
            format!("{} (exit {})\n{tail}", if o.status.success() { "✅ selesai" } else { "❌ gagal" }, o.status.code().unwrap_or(-1))
        }
    }
}

async fn git_args(dir: &Path, args: &[&str]) -> (bool, String) {
    let mut c = tokio::process::Command::new("git");
    c.args(args).current_dir(dir).kill_on_drop(true).stdin(std::process::Stdio::null());
    #[cfg(windows)]
    {
        c.creation_flags(0x0800_0000);
    }
    match tokio::time::timeout(Duration::from_secs(120), c.output()).await {
        Ok(Ok(o)) => (o.status.success(), format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)).trim().to_string()),
        _ => (false, "git tidak merespons/ tidak terpasang".into()),
    }
}

/// `/makro` -> daftar; `/makro build`; `/makro commit pesan` (git add -A, commit, push; tanpa shell, tanpa force).
pub async fn run(arg: &str) -> String {
    let Some(dir) = crate::proyek::active() else { return "Belum ada proyek. Daftarkan: /proyek C:/kode/app".into() };
    let (name, rest) = arg.trim().split_once(' ').map(|(a, b)| (a, b.trim())).unwrap_or((arg.trim(), ""));
    let list = macros_for(&dir);
    if name.is_empty() {
        let mut s = format!("Makro untuk {}:\n", dir.display());
        for (n, c) in &list {
            s.push_str(&format!("• /makro {n} → {c}\n"));
        }
        s.push_str("• /makro commit <pesan> → git add -A, commit, push\nTambah makro sendiri: makro.json di folder data ({\"nama\":\"perintah\"}).");
        return s;
    }
    if name == "commit" {
        if rest.is_empty() {
            return "Tulis pesan commit: /makro commit perbaiki bug login".into();
        }
        let (ok1, o1) = git_args(&dir, &["add", "-A"]).await;
        if !ok1 {
            return format!("❌ git add gagal: {o1}");
        }
        let (ok2, o2) = git_args(&dir, &["commit", "-m", rest]).await;
        if !ok2 {
            return format!("❌ git commit gagal (mungkin tidak ada perubahan): {o2}");
        }
        let (ok3, o3) = git_args(&dir, &["push"]).await;
        return format!("{}\n{}\n{}", o2.lines().next().unwrap_or(""), if ok3 { "✅ push berhasil".to_string() } else { "⚠ commit tersimpan lokal, push gagal:".to_string() }, o3.lines().take(4).collect::<Vec<_>>().join("\n"));
    }
    match list.iter().find(|(n, _)| n == name) {
        Some((_, cmd)) => format!("▶ {cmd}\n{}", exec(cmd, &dir, 180).await),
        None => format!("Makro '{name}' tidak ada. Ketik /makro untuk daftar."),
    }
}

// ---------- Port ----------

const PROTECTED: &[&str] = &["system", "svchost.exe", "lsass.exe", "wininit.exe", "csrss.exe", "services.exe", "smss.exe", "explorer.exe", "winlogon.exe", "dwm.exe", "nova.exe", "system idle process"];

fn listening() -> Vec<(u16, u32)> {
    let mut c = std::process::Command::new("netstat");
    c.args(["-ano", "-p", "TCP"]);
    no_window(&mut c);
    let Ok(o) = c.output() else { return vec![] };
    let mut v: Vec<(u16, u32)> = String::from_utf8_lossy(&o.stdout)
        .lines()
        .filter(|l| l.contains("LISTENING"))
        .filter_map(|l| {
            let p: Vec<&str> = l.split_whitespace().collect();
            let port = p.get(1)?.rsplit(':').next()?.parse().ok()?;
            Some((port, p.get(4)?.parse().ok()?))
        })
        .collect();
    v.sort();
    v.dedup();
    v
}

fn proc_name(pid: u32) -> String {
    let mut sys = sysinfo::System::new();
    let p = sysinfo::Pid::from_u32(pid);
    sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[p]), true);
    sys.process(p).map(|x| x.name().to_string_lossy().to_string()).unwrap_or_else(|| "?".into())
}

pub fn ports() -> String {
    let l = listening();
    if l.is_empty() {
        return "Tidak ada port yang sedang listening.".into();
    }
    let mut s = String::from("Port listening (port → proses):\n");
    for (port, pid) in l.iter().take(25) {
        s.push_str(&format!("• {port} → {} (pid {pid})\n", proc_name(*pid)));
    }
    s.push_str("Matikan yang nyangkut: /killport <nomor>");
    s
}

pub fn killport(arg: &str) -> String {
    let Ok(port) = arg.trim().parse::<u16>() else { return "Pakai: /killport 3000".into() };
    let pids: Vec<u32> = listening().into_iter().filter(|(p, _)| *p == port).map(|x| x.1).collect();
    if pids.is_empty() {
        return format!("Tidak ada proses yang listening di port {port}.");
    }
    let mut out = vec![];
    let mut sys = sysinfo::System::new();
    for pid in pids {
        let p = sysinfo::Pid::from_u32(pid);
        sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[p]), true);
        let Some(proc) = sys.process(p) else { continue };
        let name = proc.name().to_string_lossy().to_lowercase();
        if pid <= 4 || pid == std::process::id() || PROTECTED.contains(&name.as_str()) {
            out.push(format!("⛔ {name} (pid {pid}) dilindungi, tidak dimatikan"));
        } else if proc.kill() {
            out.push(format!("✅ {name} (pid {pid}) dimatikan, port {port} bebas"));
        } else {
            out.push(format!("⚠ gagal mematikan {name} (pid {pid}); coba terminal admin"));
        }
    }
    out.join("\n")
}

// ---------- Scaffolder ----------

fn words(s: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    let mut cur = String::new();
    let mut prev_lower = false;
    for ch in s.chars() {
        if ch == '-' || ch == '_' || ch == ' ' {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            prev_lower = false;
        } else {
            if ch.is_uppercase() && prev_lower && !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            prev_lower = ch.is_lowercase();
            cur.push(ch.to_ascii_lowercase());
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

pub fn expand(s: &str, name: &str) -> String {
    let w = words(name);
    let pascal: String = w.iter().map(|x| format!("{}{}", x[..1].to_uppercase(), &x[1..])).collect();
    s.replace("{{Name}}", &pascal).replace("{{name}}", &w.join("-")).replace("{{snake}}", &w.join("_")).replace("{{NAME}}", &w.join("_").to_uppercase())
}

type Tpl = &'static [(&'static str, &'static str)];

const REACT: Tpl = &[
    ("{{Name}}/{{Name}}.tsx", "import React from 'react';\nimport './{{Name}}.css';\n\nexport interface {{Name}}Props {\n  title?: string;\n}\n\n/** {{Name}}: deskripsikan komponen ini. */\nexport function {{Name}}({ title = '{{Name}}' }: {{Name}}Props) {\n  return (\n    <div className=\"{{name}}\">\n      <h2>{title}</h2>\n    </div>\n  );\n}\n\nexport default {{Name}};\n"),
    ("{{Name}}/{{Name}}.css", ".{{name}} {\n  padding: 1rem;\n}\n"),
    ("{{Name}}/index.ts", "export { default } from './{{Name}}';\nexport * from './{{Name}}';\n"),
];
const PY: Tpl = &[
    ("{{snake}}/__init__.py", "\"\"\"Paket {{snake}}.\"\"\"\n\nfrom .core import jalankan\n\n__all__ = [\"jalankan\"]\n"),
    ("{{snake}}/core.py", "\"\"\"Logika utama {{snake}}.\"\"\"\n\n\ndef jalankan(data):\n    \"\"\"Proses `data` dan kembalikan hasilnya.\"\"\"\n    return data\n"),
    ("tests/test_{{snake}}.py", "from {{snake}} import jalankan\n\n\ndef test_jalankan():\n    \"\"\"Kasus dasar.\"\"\"\n    assert jalankan(1) == 1\n"),
];
const RS: Tpl = &[("src/{{snake}}.rs", "//! Modul {{snake}}. Tambahkan `mod {{snake}};` di main.rs/lib.rs.\n\n/// Jalankan logika utama.\npub fn jalankan(x: i32) -> i32 {\n    x\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn dasar() {\n        assert_eq!(jalankan(1), 1);\n    }\n}\n")];
const EXPRESS: Tpl = &[("routes/{{name}}.js", "const express = require('express');\nconst router = express.Router();\n\n/** GET /{{name}} */\nrouter.get('/', (req, res) => {\n  res.json({ ok: true });\n});\n\nmodule.exports = router;\n")];
const HTML: Tpl = &[
    ("{{name}}/index.html", "<!doctype html>\n<html lang=\"id\">\n<head>\n  <meta charset=\"utf-8\">\n  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n  <title>{{Name}}</title>\n  <link rel=\"stylesheet\" href=\"style.css\">\n</head>\n<body>\n  <h1>{{Name}}</h1>\n  <script src=\"main.js\"></script>\n</body>\n</html>\n"),
    ("{{name}}/style.css", "body { font-family: system-ui, sans-serif; margin: 2rem; }\n"),
    ("{{name}}/main.js", "document.addEventListener('DOMContentLoaded', () => {\n  console.log('{{Name}} siap');\n});\n"),
];

fn builtin(t: &str) -> Option<Tpl> {
    Some(match t {
        "react" => REACT,
        "py" | "python" => PY,
        "rs" | "rust" => RS,
        "express" => EXPRESS,
        "html" => HTML,
        _ => return None,
    })
}

#[derive(Deserialize, Default)]
struct Aturan {
    #[serde(default)]
    header: HashMap<String, String>,
}

fn ident(s: &str) -> Option<String> {
    (!s.is_empty() && s.len() <= 40 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')).then(|| s.to_string())
}

/// Kumpulkan berkas template kustom dari templat/<nama>/ (teks, <200KB).
fn custom_tpl(t: &str) -> Option<Vec<(String, String)>> {
    let root = data_dir().join("templat").join(t);
    if !root.is_dir() || ident(t).is_none() {
        return None;
    }
    fn walk(d: &Path, root: &Path, out: &mut Vec<(String, String)>) {
        if let Ok(rd) = std::fs::read_dir(d) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, root, out);
                } else if e.metadata().map(|m| m.len() < 200_000).unwrap_or(false) {
                    if let (Ok(txt), Ok(rel)) = (std::fs::read_to_string(&p), p.strip_prefix(root)) {
                        out.push((rel.to_string_lossy().replace('\\', "/"), txt));
                    }
                }
            }
        }
    }
    let mut v = vec![];
    walk(&root, &root, &mut v);
    Some(v)
}

/// `/scaffold <template> <Nama> [folder]`
pub fn scaffold(arg: &str) -> String {
    let parts: Vec<&str> = arg.split_whitespace().collect();
    if parts.len() < 2 {
        let custom: Vec<String> = std::fs::read_dir(data_dir().join("templat")).map(|rd| rd.flatten().filter(|e| e.path().is_dir()).map(|e| e.file_name().to_string_lossy().to_string()).collect()).unwrap_or_default();
        return format!("Pakai: /scaffold <template> <Nama> [folder]\nTemplate: react (komponen), py (paket + tes), rs (modul Rust), express (route), html (halaman){}\nBuat sendiri: folder templat/<nama>/ di folder data, dengan {{{{Name}}}}/{{{{name}}}}/{{{{snake}}}} pada nama file dan isi.", if custom.is_empty() { String::new() } else { format!(", kustom: {}", custom.join(", ")) });
    }
    let Some(name) = ident(parts[1]) else { return "Nama hanya boleh huruf, angka, '-' dan '_' (maks 40).".into() };
    let base: PathBuf = match parts.get(2) {
        Some(f) => PathBuf::from(f.trim_matches('"')),
        None => match crate::proyek::active() {
            Some(p) => p,
            None => return "Belum ada proyek aktif. Daftarkan dulu: /proyek C:/kode/app, atau tulis folder tujuan.".into(),
        },
    };
    if !base.is_dir() {
        return format!("Folder tujuan tidak ada: {}", base.display());
    }
    let files: Vec<(String, String)> = match builtin(parts[0]) {
        Some(t) => t.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect(),
        None => match custom_tpl(parts[0]) {
            Some(v) if !v.is_empty() => v,
            _ => return format!("Template '{}' tidak dikenal. Ketik /scaffold untuk daftar.", parts[0]),
        },
    };
    let rules: Aturan = std::fs::read_to_string(data_dir().join("aturan.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let (mut made, mut skipped) = (vec![], vec![]);
    for (rel, body) in files {
        let rel = expand(&rel, &name);
        if rel.contains("..") || Path::new(&rel).is_absolute() {
            continue;
        }
        let path = base.join(&rel);
        if path.exists() {
            skipped.push(rel);
            continue;
        }
        let ext = path.extension().and_then(|x| x.to_str()).unwrap_or("").to_string();
        let mut content = expand(&body, &name);
        if let Some(h) = rules.header.get(&ext) {
            content = format!("{h}\n{content}");
        }
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if std::fs::write(&path, content).is_ok() {
            made.push(rel);
        }
    }
    format!("✅ Dibuat di {}: {}{}", base.display(), if made.is_empty() { "(tidak ada)".into() } else { made.join(", ") }, if skipped.is_empty() { String::new() } else { format!("\n⏭ Dilewati (sudah ada): {}", skipped.join(", ")) })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn name_expansion() {
        assert_eq!(expand("{{Name}}|{{name}}|{{snake}}|{{NAME}}", "user-card"), "UserCard|user-card|user_card|USER_CARD");
        assert_eq!(expand("{{Name}}", "UserCard"), "UserCard");
        assert!(ident("a/b").is_none() && ident("ok_1").is_some());
    }
    #[test]
    fn scaffold_creates_without_overwriting() {
        let d = std::env::temp_dir().join("nova-scaffold-test");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let r = scaffold(&format!("react TombolBaru {}", d.display()));
        assert!(r.contains("TombolBaru/TombolBaru.tsx"), "{r}");
        assert!(std::fs::read_to_string(d.join("TombolBaru/index.ts")).unwrap().contains("./TombolBaru"));
        let r2 = scaffold(&format!("react TombolBaru {}", d.display()));
        assert!(r2.contains("Dilewati"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
