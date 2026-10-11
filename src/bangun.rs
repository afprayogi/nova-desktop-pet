//! Integrasi build: `/build [folder]` menjalankan build proyek (Cargo / npm / Python) di latar dan melapor "build selesai"
//! atau menampilkan error utamanya. Error terakhir disimpan agar bisa dijelaskan ("jelaskan error").
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static LAST_ERR: Mutex<String> = Mutex::new(String::new());

pub fn last_error() -> String {
    LAST_ERR.lock().map(|e| e.clone()).unwrap_or_default()
}

/// Perintah build menurut jenis proyek.
pub fn detect(dir: &Path) -> Option<(&'static str, Vec<&'static str>)> {
    if dir.join("Cargo.toml").exists() {
        Some(("cargo", vec!["build"]))
    } else if dir.join("package.json").exists() {
        Some(if cfg!(windows) { ("npm.cmd", vec!["run", "build"]) } else { ("npm", vec!["run", "build"]) })
    } else if dir.join("pyproject.toml").exists() || dir.join("setup.py").exists() {
        Some(("python", vec!["-m", "compileall", "-q", "."]))
    } else {
        None
    }
}

fn resolve(arg: &str) -> Option<PathBuf> {
    let a = arg.trim().trim_matches('"');
    if !a.is_empty() {
        return Some(PathBuf::from(a));
    }
    crate::proyek::active()
}

/// Jalankan build dan kembalikan (sukses, ringkasan, detik).
pub async fn run(arg: &str) -> Result<(bool, String, u64), String> {
    let dir = resolve(arg).ok_or("Tentukan folder proyek: /build <folder> (atau pilih proyek aktif dengan /proyek <folder>).")?;
    let (exe, args) = detect(&dir).ok_or_else(|| format!("Jenis proyek di {} tidak dikenali (butuh Cargo.toml, package.json, atau pyproject.toml).", dir.display()))?;
    let t0 = std::time::Instant::now();
    let mut c = tokio::process::Command::new(exe);
    c.args(&args).current_dir(&dir).kill_on_drop(true);
    #[cfg(windows)]
    {
        c.creation_flags(0x0800_0000);
    }
    let out = tokio::time::timeout(std::time::Duration::from_secs(900), c.output()).await.map_err(|_| "Build melebihi 15 menit dan dihentikan.".to_string())?.map_err(|e| format!("Gagal menjalankan {exe}: {e}"))?;
    let secs = t0.elapsed().as_secs();
    let text = format!("{}\n{}", String::from_utf8_lossy(&out.stderr), String::from_utf8_lossy(&out.stdout));
    if out.status.success() {
        if let Ok(mut e) = LAST_ERR.lock() {
            e.clear();
        }
        let warn = text.lines().filter(|l| l.trim_start().starts_with("warning")).count();
        return Ok((true, format!("{} ({exe} {}) — {warn} peringatan", dir.display(), args.join(" ")), secs));
    }
    let errs: Vec<&str> = text.lines().filter(|l| l.contains("error") || l.contains("Error") || l.contains("-->")).take(8).collect();
    let summary = if errs.is_empty() { text.lines().rev().take(8).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n") } else { errs.join("\n") };
    if let Ok(mut e) = LAST_ERR.lock() {
        *e = summary.clone();
    }
    Ok((false, summary, secs))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detects_project_kinds() {
        let d = std::env::temp_dir().join(format!("nova_build_{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        assert!(detect(&d).is_none());
        std::fs::write(d.join("Cargo.toml"), "").unwrap();
        assert_eq!(detect(&d).unwrap().0, "cargo");
        let _ = std::fs::remove_dir_all(&d);
    }
}
