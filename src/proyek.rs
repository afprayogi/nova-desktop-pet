//! Asisten koding deterministik (tanpa model): pemantau berkas proyek -> linter + pemeriksa sintaks memakai bahasa yang
//! TERPASANG, pemantau git (belum di-commit / salah branch), dan pembaca log real-time (regex). Hanya folder/berkas yang
//! Bos daftarkan sendiri lewat /proyek dan /log yang dipantau.
use crate::bahasa;
use crate::config::data_dir;
use crate::engine::{Event, Tx};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

#[derive(Default, Serialize, Deserialize, Clone)]
pub struct Reg {
    pub projects: Vec<String>,
    pub logs: Vec<String>,
}

fn reg_path() -> PathBuf {
    data_dir().join("pantau.json")
}

pub fn load() -> Reg {
    std::fs::read_to_string(reg_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save(r: &Reg) {
    let _ = std::fs::write(reg_path(), serde_json::to_string_pretty(r).unwrap_or_default());
}

/// Proyek aktif = yang terakhir didaftarkan.
pub fn active() -> Option<PathBuf> {
    load().projects.last().map(PathBuf::from).filter(|p| p.is_dir())
}

pub fn proyek_cmd(arg: &str) -> String {
    let mut r = load();
    let arg = arg.trim().trim_matches('"');
    if arg.is_empty() {
        return format!("Proyek dipantau: {}\nLog dipantau: {}\nTambah: /proyek C:/kode/app · /log C:/kode/app/server.log · hapus: /proyek hapus <path>", if r.projects.is_empty() { "(belum ada)".into() } else { r.projects.join(", ") }, if r.logs.is_empty() { "(belum ada)".into() } else { r.logs.join(", ") });
    }
    if let Some(p) = arg.strip_prefix("hapus ") {
        r.projects.retain(|x| x != p.trim());
        save(&r);
        return "Proyek dihapus dari pantauan.".into();
    }
    let p = Path::new(arg);
    if !p.is_dir() {
        return format!("Folder tidak ditemukan: {arg}");
    }
    r.projects.retain(|x| x != arg);
    r.projects.push(arg.to_string());
    save(&r);
    format!("Memantau proyek: {arg}. Aku akan memeriksa file yang kamu simpan, status git, dan menyiapkan makro (/makro).")
}

pub fn log_cmd(arg: &str) -> String {
    let mut r = load();
    let arg = arg.trim().trim_matches('"');
    if let Some(p) = arg.strip_prefix("hapus ") {
        r.logs.retain(|x| x != p.trim());
        save(&r);
        return "Log dihapus dari pantauan.".into();
    }
    if arg.is_empty() || !Path::new(arg).is_file() {
        return "Beri path file log yang ada: /log C:/kode/app/server.log".into();
    }
    r.logs.retain(|x| x != arg);
    r.logs.push(arg.to_string());
    save(&r);
    format!("Memantau log: {arg} (baris error baru akan kulaporkan).")
}

// ---------- Linter deterministik ----------

#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    pub line: Option<usize>,
    pub msg: String,
}

fn issue(line: usize, msg: impl Into<String>) -> Issue {
    Issue { line: Some(line), msg: msg.into() }
}

pub fn lint_text(ext: &str, text: &str) -> Vec<Issue> {
    let mut v = vec![];
    let lines: Vec<&str> = text.lines().collect();
    let mut long = 0;
    for (i, l) in lines.iter().enumerate() {
        if l.starts_with("<<<<<<<") || l.starts_with(">>>>>>>") {
            v.push(issue(i + 1, "penanda konflik merge belum diselesaikan"));
        }
        if l.len() > 200 {
            long += 1;
        }
    }
    match ext {
        "py" => {
            let def = Regex::new(r"^\s*(?:async\s+)?(?:def|class)\s+([A-Za-z_]\w*)").unwrap();
            for (i, l) in lines.iter().enumerate() {
                if let Some(c) = def.captures(l) {
                    let name = &c[1];
                    if name.starts_with('_') || name.starts_with("test") {
                        continue;
                    }
                    // cari akhir tanda tangan (baris yang berakhir ':'), lalu baris tubuh pertama
                    let mut j = i;
                    while j < lines.len() && !lines[j].trim_end().ends_with(':') && j < i + 8 {
                        j += 1;
                    }
                    let body = lines.iter().skip(j + 1).map(|b| b.trim()).find(|b| !b.is_empty() && !b.starts_with('#'));
                    if let Some(b) = body {
                        if !(b.starts_with("\"\"\"") || b.starts_with("'''") || b.starts_with('"') || b.starts_with('\'')) {
                            v.push(issue(i + 1, format!("'{name}' tanpa docstring")));
                        }
                    }
                }
                let t = l.trim();
                if t == "except:" || t.starts_with("except:") {
                    v.push(issue(i + 1, "'except:' kosong menelan semua error; sebut tipe exception-nya"));
                }
                if Regex::new(r"def\s+\w+\(.*=\s*(\[\]|\{\})").unwrap().is_match(l) {
                    v.push(issue(i + 1, "argumen default mutable ([] atau {}) dibagi antar pemanggilan"));
                }
            }
        }
        "js" | "mjs" | "cjs" | "jsx" | "ts" | "tsx" => {
            let mut logs = 0;
            for (i, l) in lines.iter().enumerate() {
                let t = l.trim_start();
                if t.starts_with("var ") {
                    v.push(issue(i + 1, "gunakan let/const, bukan var"));
                }
                if l.contains("console.log(") {
                    logs += 1;
                }
                if ext.starts_with("ts") && l.contains(": any") {
                    v.push(issue(i + 1, "tipe 'any' meniadakan keamanan tipe TypeScript"));
                }
            }
            if logs > 3 {
                v.push(Issue { line: None, msg: format!("{logs} console.log tertinggal; hapus sebelum rilis") });
            }
        }
        "rs" => {
            let mut unwraps = 0;
            for (i, l) in lines.iter().enumerate() {
                if l.contains("todo!(") || l.contains("unimplemented!(") {
                    v.push(issue(i + 1, "todo!/unimplemented! akan panic saat dijalankan"));
                }
                if l.contains("dbg!(") {
                    v.push(issue(i + 1, "dbg! tertinggal"));
                }
                unwraps += l.matches(".unwrap()").count();
            }
            if unwraps > 8 {
                v.push(Issue { line: None, msg: format!("{unwraps}x .unwrap() — pertimbangkan ? atau penanganan error") });
            }
        }
        _ => {}
    }
    if long > 0 {
        v.push(Issue { line: None, msg: format!("{long} baris lebih dari 200 karakter") });
    }
    v.truncate(6);
    v
}

fn run_cmd(prog: &str, args: &[&str], cwd: Option<&Path>) -> Option<(bool, String)> {
    let mut c = Command::new(prog);
    c.args(args).stdin(Stdio::null());
    if let Some(d) = cwd {
        c.current_dir(d);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let o = c.output().ok()?;
    Some((o.status.success(), format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))))
}

fn line_no(s: &str) -> Option<usize> {
    Regex::new(r"(?:line |:)(\d{1,6})").ok()?.captures(s)?.get(1)?.as_str().parse().ok()
}

/// Pemeriksa sintaks memakai bahasa yang terpasang di laptop. None = tidak ada kesalahan / alat tidak ada.
pub fn syntax_check(path: &Path, ext: &str) -> Option<Issue> {
    let p = path.to_string_lossy().to_string();
    let (ok, out) = match ext {
        "json" => {
            let t = std::fs::read_to_string(path).ok()?;
            return serde_json::from_str::<serde_json::Value>(&t).err().map(|e| Issue { line: Some(e.line()), msg: format!("JSON tidak valid: {e}") });
        }
        "py" if bahasa::has("python") => run_cmd(&bahasa::cmd_of("python")?, &["-c", "import ast,sys;ast.parse(open(sys.argv[1],encoding='utf-8').read(),sys.argv[1])", &p], None)?,
        "js" | "mjs" | "cjs" if bahasa::has("node") => run_cmd("node", &["--check", &p], None)?,
        "go" if bahasa::has("go") => run_cmd("gofmt", &["-e", &p], None).filter(|r| !r.1.is_empty()).or(Some((true, String::new())))?,
        "rs" if bahasa::has("rust") => {
            let (ok, out) = run_cmd("rustfmt", &["--edition", "2021", "--check", &p], None)?;
            // selisih format bukan kesalahan sintaks
            (ok || !out.contains("error:") || out.contains("Diff in"), out)
        }
        "c" | "cpp" | "h" | "hpp" if bahasa::has("c") || bahasa::has("cpp") => run_cmd(&bahasa::cmd_of(if ext.starts_with('c') && ext != "cpp" { "c" } else { "cpp" }).unwrap_or("gcc".into()), &["-fsyntax-only", &p], None)?,
        "php" if bahasa::has("php") => run_cmd("php", &["-l", &p], None)?,
        "rb" if bahasa::has("ruby") => run_cmd("ruby", &["-c", &p], None)?,
        "sh" if bahasa::has("bash") => run_cmd("bash", &["-n", &p], None)?,
        "ps1" if bahasa::has("powershell") => {
            let script = format!("$e=$null;[void][System.Management.Automation.Language.Parser]::ParseFile('{}',[ref]$null,[ref]$e);$e|%{{\"$($_.Extent.StartLineNumber): $($_.Message)\"}}", p.replace('\'', "''"));
            let (_, out) = run_cmd("powershell", &["-NoProfile", "-Command", &script], None)?;
            (out.trim().is_empty(), out)
        }
        _ => return None,
    };
    if ok {
        return None;
    }
    let msg = out.lines().map(|l| l.trim()).rev().find(|l| l.contains("rror") || l.contains("xpected") || l.contains("invalid") || l.contains("Unexpected")).or_else(|| out.lines().find(|l| !l.trim().is_empty())).unwrap_or("kesalahan sintaks").to_string();
    Some(Issue { line: line_no(&out), msg: format!("sintaks: {}", msg.chars().take(110).collect::<String>()) })
}

// ---------- Pemantau ----------

const SKIP: &[&str] = &[".git", "node_modules", "target", ".venv", "venv", "__pycache__", "dist", "build", ".next", ".idea", ".vscode", "bin", "obj"];
const EXT: &[&str] = &["py", "js", "mjs", "cjs", "jsx", "ts", "tsx", "rs", "go", "json", "ps1", "c", "cpp", "h", "hpp", "php", "rb", "sh"];

fn scan(root: &Path, depth: u8, out: &mut HashMap<PathBuf, SystemTime>) {
    if depth > 6 || out.len() > 6000 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(root) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() {
            if !SKIP.contains(&name.as_str()) && !name.starts_with('.') {
                scan(&p, depth + 1, out);
            }
        } else if p.extension().and_then(|x| x.to_str()).is_some_and(|x| EXT.contains(&x)) {
            if let Ok(m) = e.metadata().and_then(|m| m.modified()) {
                out.insert(p, m);
            }
        }
    }
}

#[derive(Default)]
struct GitSt {
    dirty_since: Option<Instant>,
    last_nudge: Option<Instant>,
    main_warned_day: String,
}

#[derive(Default)]
pub struct State {
    files: HashMap<String, HashMap<PathBuf, SystemTime>>,
    baselined: std::collections::HashSet<String>,
    last_notice: HashMap<PathBuf, Instant>,
    recent_change: HashMap<String, Instant>,
    git: HashMap<String, GitSt>,
    log_off: HashMap<String, u64>,
    last_log_note: Option<Instant>,
}

pub struct Note {
    pub lines: Vec<(usize, String)>,
    pub mood: Option<&'static str>,
}

fn short(p: &Path, root: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).to_string_lossy().replace('\\', "/")
}

fn log_pattern() -> Regex {
    Regex::new(r"(?i)\b(TypeError|ReferenceError|SyntaxError|NullPointerException|Traceback \(most recent|panicked at|segmentation fault|Unhandled(?:Promise)?Rejection|ECONNREFUSED|database connection failed|OutOfMemory|stack overflow|FATAL|Exception in thread)\b").unwrap()
}

/// Satu putaran (blocking): berkas berubah -> periksa; git tiap ~60 dtk; log baru.
pub fn round(st: &mut State, tick: u64) -> Vec<Note> {
    let reg = load();
    let mut notes = vec![];
    for proj in &reg.projects {
        let root = PathBuf::from(proj);
        if !root.is_dir() {
            continue;
        }
        let mut now_files = HashMap::new();
        scan(&root, 0, &mut now_files);
        let first = st.baselined.insert(proj.clone());
        let old = st.files.entry(proj.clone()).or_default();
        let mut changed: Vec<PathBuf> = vec![];
        if !first {
            for (p, m) in &now_files {
                if old.get(p).is_none_or(|o| o != m) {
                    changed.push(p.clone());
                }
            }
        }
        *old = now_files;
        for p in changed.into_iter().take(3) {
            st.recent_change.insert(proj.clone(), Instant::now());
            if st.last_notice.get(&p).is_some_and(|t| t.elapsed() < Duration::from_secs(20)) {
                continue;
            }
            st.last_notice.insert(p.clone(), Instant::now());
            let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("").to_string();
            let mut issues: Vec<Issue> = syntax_check(&p, &ext).into_iter().collect();
            if issues.is_empty() {
                if let Ok(t) = std::fs::read_to_string(&p) {
                    if t.len() < 1_000_000 {
                        issues = lint_text(&ext, &t);
                    }
                }
            }
            if let Some(first) = issues.first() {
                let f = short(&p, &root);
                let at = first.line.map(|l| format!(" baris {l}")).unwrap_or_default();
                let more = if issues.len() > 1 { format!(" (+{} temuan lain)", issues.len() - 1) } else { String::new() };
                let syntax = first.msg.starts_with("sintaks");
                notes.push(Note { lines: vec![(1, format!("{f}{at}: {}{more}", first.msg))], mood: syntax.then_some("pusing") });
            }
        }
        // Git tiap ~60 detik (tick 5 dtk)
        if tick % 12 == 3 && root.join(".git").exists() && bahasa::has("git") {
            let name = root.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let dirty = run_cmd("git", &["status", "--porcelain"], Some(&root)).map(|r| !r.1.trim().is_empty()).unwrap_or(false);
            let branch = run_cmd("git", &["branch", "--show-current"], Some(&root)).map(|r| r.1.trim().to_string()).unwrap_or_default();
            let g = st.git.entry(proj.clone()).or_default();
            if dirty {
                let since = *g.dirty_since.get_or_insert_with(Instant::now);
                if since.elapsed() >= Duration::from_secs(7200) && g.last_nudge.is_none_or(|t| t.elapsed() > Duration::from_secs(7200)) {
                    g.last_nudge = Some(Instant::now());
                    notes.push(Note { lines: vec![(2, format!("Sudah {} jam ada perubahan belum di-commit di '{name}'. Commit kecil lebih aman: /makro commit <pesan>", since.elapsed().as_secs() / 3600))], mood: Some("helm") });
                }
            } else {
                g.dirty_since = None;
            }
            let today = chrono::Local::now().format("%Y-%m-%d").to_string();
            let recent = st.recent_change.get(proj).is_some_and(|t| t.elapsed() < Duration::from_secs(900));
            let g = st.git.entry(proj.clone()).or_default();
            if (branch == "main" || branch == "master") && dirty && recent && g.main_warned_day != today {
                g.main_warned_day = today;
                notes.push(Note { lines: vec![(2, format!("Hei, kamu masih di branch {branch} pada '{name}'! Buat branch baru dulu: git switch -c fitur-baru"))], mood: Some("helm") });
            }
        }
    }
    // Log real-time
    for lg in &reg.logs {
        let Ok(mut f) = std::fs::File::open(lg) else { continue };
        let len = f.metadata().map(|m| m.len()).unwrap_or(0);
        let off = st.log_off.entry(lg.clone()).or_insert(len); // mulai dari ujung file
        if len < *off {
            *off = 0;
        }
        if len == *off {
            continue;
        }
        let start = (*off).max(len.saturating_sub(65536));
        let _ = f.seek(SeekFrom::Start(start));
        let mut buf = String::new();
        let _ = f.take(65536).read_to_string(&mut buf);
        *off = len;
        if let Some(l) = buf.lines().find(|l| log_pattern().is_match(l)) {
            if st.last_log_note.is_none_or(|t| t.elapsed() > Duration::from_secs(30)) {
                st.last_log_note = Some(Instant::now());
                let name = Path::new(lg).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                let mut lines = vec![(2, format!("Log {name}: {}", l.trim().chars().take(110).collect::<String>()))];
                if let Some(sol) = crate::solusi::cari(l) {
                    lines.push((1, sol));
                }
                notes.push(Note { lines, mood: Some("pusing") });
            }
        }
    }
    notes
}

pub async fn run(tx: Tx) {
    let _ = tokio::task::spawn_blocking(bahasa::detect).await; // bahasa terpasang -> pemeriksa sintaks aktif
    let mut st = State::default();
    let mut tick = 0u64;
    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let reg = load();
        if reg.projects.is_empty() && reg.logs.is_empty() {
            continue;
        }
        let t = tick;
        let (st2, notes) = tokio::task::spawn_blocking(move || {
            let n = round(&mut st, t);
            (st, n)
        })
        .await
        .unwrap_or_else(|_| (State::default(), vec![]));
        st = st2;
        tick += 1;
        for n in notes.into_iter().take(2) {
            if let Some(m) = n.mood {
                tx.send(Event::Mood { emo: m.into(), level: 0.9 });
            }
            crate::notify::toast("NOVA - kode", &n.lines[0].1);
            crate::adegan::play(&tx, n.lines);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lints() {
        let py = "def hitung(a):\n    return a\n\ndef ok(b):\n    \"\"\"doc\"\"\"\n    return b\n\ntry:\n    pass\nexcept:\n    pass\n";
        let v = lint_text("py", py);
        assert!(v.iter().any(|i| i.msg.contains("'hitung' tanpa docstring") && i.line == Some(1)));
        assert!(!v.iter().any(|i| i.msg.contains("'ok'")));
        assert!(v.iter().any(|i| i.msg.contains("except")));
        let js = "var a = 1;\nconsole.log(1);console.log(2);\n";
        assert!(lint_text("js", js).iter().any(|i| i.msg.contains("let/const")));
        assert!(lint_text("rs", "fn x(){ todo!() }").iter().any(|i| i.msg.contains("todo!")));
        assert!(lint_text("txt", "<<<<<<< HEAD").iter().any(|i| i.msg.contains("konflik")));
    }
    #[test]
    fn json_syntax_and_log_pattern() {
        let d = std::env::temp_dir().join("nova-test-json.json");
        std::fs::write(&d, "{\"a\": 1,, }").unwrap();
        let i = syntax_check(&d, "json").unwrap();
        assert!(i.msg.contains("JSON") && i.line == Some(1));
        let _ = std::fs::remove_file(&d);
        assert!(log_pattern().is_match("12:00 java.lang.NullPointerException at X"));
        assert!(!log_pattern().is_match("semua baik"));
    }
}
