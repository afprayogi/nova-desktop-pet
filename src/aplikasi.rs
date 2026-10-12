//! Indeks aplikasi terpasang (menu Start: pintasan .lnk + aplikasi Store/UWP) agar "buka kicad/autocad/bleachbit" berfungsi
//! untuk apa pun yang ada di laptop, bukan hanya daftar tetap. Cocok-kata-kabur; hasil di-cache 10 menit.
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct App {
    pub nama: String,
    /// Berkas .lnk, atau `shell:AppsFolder\<AppID>` untuk aplikasi Store.
    pub target: String,
}

static CACHE: Mutex<Option<(Instant, Vec<App>)>> = Mutex::new(None);

fn walk(dir: &std::path::Path, depth: u32, out: &mut Vec<App>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() && depth < 3 {
            walk(&p, depth + 1, out);
        } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("lnk")) {
            let nama = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let l = nama.to_lowercase();
            if !["uninstall", "hapus", "remove", "readme", "license", "manual", "help", "documentation"].iter().any(|k| l.contains(k)) {
                out.push(App { nama, target: p.to_string_lossy().into_owned() });
            }
        }
    }
}

fn scan() -> Vec<App> {
    let mut v = vec![];
    for var in ["ProgramData", "APPDATA"] {
        if let Ok(b) = std::env::var(var) {
            walk(&std::path::Path::new(&b).join("Microsoft/Windows/Start Menu/Programs"), 0, &mut v);
        }
    }
    // Aplikasi Store/UWP: Get-StartApps -> "Nama\tAppID"
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        if let Ok(o) = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", "Get-StartApps | ForEach-Object { $_.Name + \"`t\" + $_.AppID }"])
            .creation_flags(0x0800_0000)
            .output()
        {
            for l in String::from_utf8_lossy(&o.stdout).lines() {
                if let Some((n, id)) = l.split_once('\t') {
                    if !n.trim().is_empty() && id.contains('!') {
                        v.push(App { nama: n.trim().to_string(), target: format!("shell:AppsFolder\\{}", id.trim()) });
                    }
                }
            }
        }
    }
    v.sort_by(|a, b| a.nama.to_lowercase().cmp(&b.nama.to_lowercase()));
    v.dedup_by(|a, b| a.nama.eq_ignore_ascii_case(&b.nama));
    v
}

pub fn daftar() -> Vec<App> {
    let mut c = CACHE.lock().unwrap();
    if let Some((t, v)) = c.as_ref() {
        if t.elapsed() < Duration::from_secs(600) {
            return v.clone();
        }
    }
    let v = scan();
    *c = Some((Instant::now(), v.clone()));
    v
}

fn kata(s: &str) -> Vec<String> {
    s.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty() && !["aplikasi", "program", "app", "software", "the"].contains(w)).map(String::from).collect()
}

fn skor(q: &[String], nama: &str) -> u32 {
    let n = kata(nama);
    let (qj, nj) = (q.concat(), n.concat());
    if qj.is_empty() {
        return 0;
    }
    if qj == nj {
        return 100;
    }
    if q.iter().all(|w| n.iter().any(|x| x.starts_with(w.as_str()))) {
        return 90 - (n.len().saturating_sub(q.len()) as u32).min(30);
    }
    if nj.contains(&qj) && qj.len() >= 4 {
        return 60;
    }
    0
}

pub fn cari_dalam(apps: &[App], q: &str) -> Option<App> {
    let q = kata(q);
    apps.iter().map(|a| (skor(&q, &a.nama), a)).filter(|(s, _)| *s > 0).max_by_key(|(s, a)| (*s, std::cmp::Reverse(a.nama.len()))).map(|(_, a)| a.clone())
}

pub fn cari(q: &str) -> Option<App> {
    cari_dalam(&daftar(), q)
}

pub fn buka(a: &App) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let r = if a.target.starts_with("shell:") {
            std::process::Command::new("explorer.exe").arg(&a.target).creation_flags(0x0800_0000).spawn()
        } else {
            let mut c = std::process::Command::new("cmd");
            c.raw_arg(format!("/C start \"\" \"{}\"", a.target)).creation_flags(0x0800_0000);
            c.spawn()
        };
        return r.is_ok();
    }
    #[allow(unreachable_code)]
    false
}

/// `/aplikasi [kata]`: daftar aplikasi terpasang (disaring bila ada kata).
pub fn daftar_teks(q: &str) -> String {
    let all = daftar();
    let kk = kata(q);
    let v: Vec<&str> = all.iter().filter(|a| kk.is_empty() || skor(&kk, &a.nama) > 0 || a.nama.to_lowercase().contains(&q.to_lowercase())).map(|a| a.nama.as_str()).collect();
    if v.is_empty() {
        return format!("Tidak ada aplikasi bernama '{q}' di menu Start ({} aplikasi terindeks).", all.len());
    }
    format!("{} aplikasi{}: {}", v.len(), if q.is_empty() { " terpasang".to_string() } else { format!(" cocok '{q}'") }, v.iter().take(60).cloned().collect::<Vec<_>>().join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn a(n: &str) -> App {
        App { nama: n.into(), target: n.into() }
    }
    #[test]
    fn fuzzy_matching() {
        let v = vec![a("KiCad 8.0"), a("AutoCAD 2024 - English"), a("BleachBit"), a("Google Chrome"), a("YouTube Music"), a("Chrome Remote Desktop")];
        assert_eq!(cari_dalam(&v, "kicad").unwrap().nama, "KiCad 8.0");
        assert_eq!(cari_dalam(&v, "autocad").unwrap().nama, "AutoCAD 2024 - English");
        assert_eq!(cari_dalam(&v, "bleach bit").unwrap().nama, "BleachBit");
        assert_eq!(cari_dalam(&v, "chrome").unwrap().nama, "Google Chrome");
        assert!(cari_dalam(&v, "photoshop").is_none());
    }
}
