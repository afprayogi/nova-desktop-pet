//! Pembantu kerja engineer: saran sadar-proyek (git/TODO/build), kalkulator kelistrikan tambahan, waktu per proyek, dan ekspor laporan.
//! Semuanya lokal dari data yang sudah ada (git, berkas proyek aktif, nama aplikasi); tidak membaca isi layar atau ketikan.
use crate::config::data_dir;
use chrono::Local;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

// ---------- waktu per proyek ----------

fn berkas_waktu() -> PathBuf {
    data_dir().join("waktu_proyek.json")
}

fn muat_waktu() -> BTreeMap<String, BTreeMap<String, i64>> {
    std::fs::read_to_string(berkas_waktu()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

static TERAKHIR: Mutex<Option<Instant>> = Mutex::new(None);

fn nama_proyek(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Tambahkan waktu sejak panggilan terakhir ke proyek aktif bila Bos sedang di mode Coding/Teknik dan tidak diam (paling lama 2 menit sekali ukur).
pub fn catat_waktu() {
    let sekarang = Instant::now();
    let lalu = TERAKHIR.lock().ok().and_then(|mut t| t.replace(sekarang));
    let Some(lalu) = lalu else { return };
    let detik = sekarang.duration_since(lalu).as_secs().min(120) as i64;
    let (mode, idle) = crate::konteks::current();
    if !matches!(mode, crate::konteks::Mode::Coding | crate::konteks::Mode::Teknik) || idle > 120 {
        return;
    }
    let Some(proyek) = crate::proyek::active().map(|p| nama_proyek(&p)).filter(|n| !n.is_empty()) else { return };
    tambah_waktu(&Local::now().format("%Y-%m-%d").to_string(), &proyek, detik);
}

fn tambah_waktu(hari: &str, proyek: &str, detik: i64) {
    let mut w = muat_waktu();
    *w.entry(hari.to_string()).or_default().entry(proyek.to_string()).or_insert(0) += detik;
    // simpan 60 hari terakhir saja
    while w.len() > 60 {
        if let Some(k) = w.keys().next().cloned() {
            w.remove(&k);
        }
    }
    let _ = std::fs::write(berkas_waktu(), serde_json::to_string(&w).unwrap_or_default());
}

fn durasi(detik: i64) -> String {
    let m = detik / 60;
    if m >= 60 { format!("{}j {}m", m / 60, m % 60) } else { format!("{m}m") }
}

/// Jumlah waktu per proyek untuk `hari` hari terakhir (1 = hari ini).
pub fn waktu_proyek(hari: i64) -> String {
    let dari = (Local::now() - chrono::Duration::days(hari - 1)).format("%Y-%m-%d").to_string();
    let mut total: BTreeMap<String, i64> = BTreeMap::new();
    for (_, per) in muat_waktu().range(dari..) {
        for (p, s) in per {
            *total.entry(p.clone()).or_insert(0) += s;
        }
    }
    let mut v: Vec<_> = total.into_iter().filter(|x| x.1 >= 60).collect();
    v.sort_by_key(|x| std::cmp::Reverse(x.1));
    if v.is_empty() {
        return format!("⏱ Belum ada waktu proyek tercatat {}. Aku mencatat saat Bos di mode Coding/Teknik dan ada proyek aktif (/proyek <folder>).", if hari == 1 { "hari ini" } else { "di rentang ini" });
    }
    format!("⏱ Waktu per proyek ({}):\n{}", if hari == 1 { "hari ini".to_string() } else { format!("{hari} hari") }, v.iter().map(|(p, s)| format!("• {p}: {}", durasi(*s))).collect::<Vec<_>>().join("\n"))
}

// ---------- saran sadar-proyek ----------

fn hitung_todo(root: &Path, kedalaman: u8, n_berkas: &mut u32, todo: &mut Vec<String>) {
    let Ok(d) = std::fs::read_dir(root) else { return };
    for e in d.flatten() {
        let p = e.path();
        let nama = e.file_name().to_string_lossy().into_owned();
        if p.is_dir() {
            if kedalaman < 4 && !matches!(nama.as_str(), "target" | "node_modules" | ".git" | "dist" | "build" | "venv" | ".venv" | "__pycache__") {
                hitung_todo(&p, kedalaman + 1, n_berkas, todo);
            }
        } else if *n_berkas < 400 && p.extension().and_then(|x| x.to_str()).is_some_and(|x| matches!(x, "rs" | "py" | "js" | "ts" | "tsx" | "c" | "cpp" | "h" | "ino" | "go" | "java" | "cs")) && e.metadata().map(|m| m.len() < 400_000).unwrap_or(false) {
            *n_berkas += 1;
            if let Ok(t) = std::fs::read_to_string(&p) {
                for (i, l) in t.lines().enumerate() {
                    if todo.len() < 200 && (l.contains("TODO") || l.contains("FIXME")) {
                        todo.push(format!("{}:{} {}", nama, i + 1, l.trim().chars().take(70).collect::<String>()));
                    }
                }
            }
        }
    }
}

pub fn saran_proyek() -> String {
    let Some(dir) = crate::proyek::active() else { return "Belum ada proyek aktif. Pilih dengan: /proyek <folder>.".into() };
    let g = |a: &[&str]| crate::ekstra::git(&dir, a);
    let nama = nama_proyek(&dir);
    let mut saran = vec![];
    let mut o = format!("🧭 Saran untuk proyek '{nama}'\n");
    let status = g(&["status", "-sb"]);
    if status.is_empty() {
        o.push_str("Bukan repositori git, jadi saran git dilewati.\n");
    } else {
        let cabang = status.lines().next().unwrap_or("").trim_start_matches("## ").to_string();
        let berubah = status.lines().skip(1).count();
        o.push_str(&format!("🌿 {cabang} · {berubah} berkas berubah\n"));
        if berubah >= 10 {
            saran.push(format!("Ada {berubah} berkas belum di-commit: pecah jadi beberapa commit kecil."));
        } else if berubah > 0 {
            saran.push("Ada perubahan belum di-commit: commit sebelum pindah tugas.".into());
        }
        if cabang.contains("[ahead") {
            saran.push("Ada commit yang belum di-push.".into());
        }
        if let Some(umur) = g(&["log", "-1", "--pretty=%ct"]).parse::<i64>().ok().map(|t| (Local::now().timestamp() - t) / 86400) {
            if berubah > 0 && umur >= 2 {
                saran.push(format!("Commit terakhir {umur} hari lalu tetapi ada perubahan menggantung."));
            }
        }
    }
    let (mut n, mut todo) = (0u32, vec![]);
    hitung_todo(&dir, 0, &mut n, &mut todo);
    if !todo.is_empty() {
        o.push_str(&format!("📝 {} TODO/FIXME di {n} berkas, mis.: {}\n", todo.len(), todo.iter().take(3).cloned().collect::<Vec<_>>().join(" | ")));
        saran.push("Pilih satu TODO tertua untuk dibereskan hari ini.".into());
    }
    let build = [("Cargo.toml", "cargo check lalu cargo test"), ("package.json", "npm test (periksa skrip di package.json)"), ("pyproject.toml", "pytest"), ("requirements.txt", "python -m pytest atau jalankan skripnya"), ("platformio.ini", "pio run"), ("CMakeLists.txt", "cmake --build")];
    if let Some((m, c)) = build.iter().find(|(m, _)| dir.join(m).exists()) {
        saran.push(format!("Terdeteksi {m}: sebelum commit jalankan {c}."));
    }
    let (mode, _) = crate::konteks::current();
    if matches!(mode, crate::konteks::Mode::Coding) {
        o.push_str(&format!("{}\n", waktu_proyek(1).lines().nth(1).map(|l| format!("⏱ Hari ini {}", l.trim_start_matches("• "))).unwrap_or_default()));
    }
    if saran.is_empty() {
        o.push_str("✅ Tidak ada yang mendesak. Lanjutkan.");
    } else {
        o.push_str(&format!("Saran:\n{}", saran.iter().map(|s| format!("  – {s}")).collect::<Vec<_>>().join("\n")));
    }
    o
}

// ---------- kalkulator teknik ----------

const RHO_CU: f64 = 0.0175; // Ω·mm²/m pada ±20°C
const RHO_AL: f64 = 0.0282;

fn angka(w: &[&str], i: usize) -> Result<f64, String> {
    w.get(i).and_then(|s| s.replace(',', ".").parse::<f64>().ok()).filter(|v| v.is_finite()).ok_or_else(|| format!("argumen ke-{} harus angka", i + 1))
}

const BANTUAN: &str = "Kalkulator teknik:\n• drop <V> <I> <panjang_m> <mm2> [al] [3f] — jatuh tegangan kabel 1 fasa (2 penghantar) atau 3 fasa\n• luas <V> <I> <panjang_m> <maks_drop_%> [al] [3f] — luas penampang minimum\n• seri <R1> <R2> ... · paralel <R1> <R2> ...\n• pf <kW> <pf_awal> <pf_target> — kapasitor koreksi faktor daya (kVAR)\n• db daya|tegangan <rasio> — desibel\nResistivitas tembaga 0,0175 / aluminium 0,0282 Ω·mm²/m (±20°C, resistif saja, belum memperhitungkan reaktansi).";

pub fn kalk(arg: &str) -> String {
    let w: Vec<&str> = arg.split_whitespace().collect();
    let Some(&cmd) = w.first() else { return BANTUAN.into() };
    let (al, tiga) = (w.iter().any(|x| x.eq_ignore_ascii_case("al")), w.iter().any(|x| x.eq_ignore_ascii_case("3f")));
    let rho = if al { RHO_AL } else { RHO_CU };
    let faktor = if tiga { 3f64.sqrt() } else { 2.0 };
    let jenis = format!("{} {}", if al { "aluminium" } else { "tembaga" }, if tiga { "3 fasa" } else { "1 fasa" });
    let hasil = (|| -> Result<String, String> {
        Ok(match cmd {
            "drop" => {
                let (v, i, l, a) = (angka(&w, 1)?, angka(&w, 2)?, angka(&w, 3)?, angka(&w, 4)?);
                if v <= 0.0 || a <= 0.0 || l < 0.0 || i < 0.0 {
                    return Err("nilai harus positif".into());
                }
                let dv = faktor * l * i * rho / a;
                let pct = dv / v * 100.0;
                format!("⚡ Jatuh tegangan ({jenis}): {dv:.2} V = {pct:.2}% dari {v:.0} V — {}", if pct <= 3.0 { "dalam batas umum ≤3% (cabang)" } else if pct <= 5.0 { "lewat 3% tetapi ≤5% (total)" } else { "MELEWATI 5%: perbesar penampang atau perpendek jarak" })
            }
            "luas" => {
                let (v, i, l, p) = (angka(&w, 1)?, angka(&w, 2)?, angka(&w, 3)?, angka(&w, 4)?);
                if v <= 0.0 || p <= 0.0 || l < 0.0 || i < 0.0 {
                    return Err("nilai harus positif".into());
                }
                let a = faktor * l * i * rho / (v * p / 100.0);
                let std = [1.5, 2.5, 4.0, 6.0, 10.0, 16.0, 25.0, 35.0, 50.0, 70.0, 95.0, 120.0, 150.0, 185.0, 240.0];
                let pilih = std.iter().find(|s| **s >= a).map(|s| format!("{s} mm²")).unwrap_or_else(|| "lebih dari 240 mm² (pakai paralel)".into());
                format!("🔌 Penampang minimum ({jenis}) agar drop ≤{p}%: {a:.2} mm² → pilih {pilih}. Periksa juga KHA (kuat hantar arus) dan pengaman; hitungan ini hanya jatuh tegangan.")
            }
            "seri" | "paralel" => {
                let r: Vec<f64> = (1..w.len()).map(|i| angka(&w, i)).collect::<Result<_, _>>()?;
                if r.len() < 2 || r.iter().any(|x| *x <= 0.0) {
                    return Err("butuh minimal dua nilai positif".into());
                }
                let t = if cmd == "seri" { r.iter().sum::<f64>() } else { 1.0 / r.iter().map(|x| 1.0 / x).sum::<f64>() };
                format!("🧮 {cmd} {} → {t:.4} (satuan sama dengan masukan)", r.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", "))
            }
            "pf" => {
                let (p, a, b) = (angka(&w, 1)?, angka(&w, 2)?, angka(&w, 3)?);
                if p <= 0.0 || !(0.1..=1.0).contains(&a) || !(0.1..=1.0).contains(&b) || b <= a {
                    return Err("kW > 0, pf awal < pf target, keduanya 0,1–1".into());
                }
                let qc = p * (a.acos().tan() - b.acos().tan());
                format!("🔋 Koreksi faktor daya {a:.2} → {b:.2} untuk {p} kW: kapasitor ≈ {qc:.2} kVAR (pilih nilai standar terdekat di atasnya; waspadai harmonisa).")
            }
            "db" => {
                let r = angka(&w, 2)?;
                if r <= 0.0 {
                    return Err("rasio harus positif".into());
                }
                let k = match w.get(1).copied() {
                    Some("daya") => 10.0,
                    Some("tegangan") => 20.0,
                    _ => return Err("pilih 'daya' atau 'tegangan'".into()),
                };
                format!("📈 Rasio {} {r} = {:.2} dB", w[1], k * r.log10())
            }
            _ => BANTUAN.to_string(),
        })
    })();
    hasil.unwrap_or_else(|e| format!("{e}\n{BANTUAN}"))
}

// ---------- ekspor laporan ----------

fn folder_laporan() -> PathBuf {
    std::env::var("USERPROFILE").map(|p| PathBuf::from(p).join("Documents").join("NOVA")).unwrap_or_else(|_| data_dir().join("laporan"))
}

/// Tulis rekap hari ini + 7 hari + waktu proyek sebagai Markdown di Dokumen\NOVA. Mengembalikan pesan berisi lokasi.
pub fn ekspor_laporan() -> String {
    let dir = folder_laporan();
    if std::fs::create_dir_all(&dir).is_err() {
        return "Gagal membuat folder laporan.".into();
    }
    let hari = Local::now().format("%Y-%m-%d").to_string();
    let md = format!("# Laporan kerja {hari}\n\n## Hari ini\n```\n{}\n```\n\n## 7 hari terakhir\n```\n{}\n```\n\n## Waktu per proyek (7 hari)\n```\n{}\n```\n\n_Dibuat NOVA {} dari data lokal (nama aplikasi, git, tugas)._\n", crate::kerja::rekap_hari(), crate::kerja::rekap_minggu(), waktu_proyek(7), Local::now().format("%H:%M"));
    let p = dir.join(format!("laporan-{hari}.md"));
    match std::fs::write(&p, md) {
        Ok(()) => format!("📄 Laporan disimpan: {}", p.display()),
        Err(e) => format!("Gagal menyimpan laporan: {e}"),
    }
}

pub fn data_waktu() -> Value {
    json!(muat_waktu())
}

// ---------- ringkasan issue GitHub (repo publik, tanpa kunci) ----------

/// "owner/repo" dari argumen atau dari remote origin proyek aktif.
pub fn repo_github(arg: &str) -> Option<String> {
    let valid = |s: &str| s.split_once('/').is_some_and(|(a, b)| !a.is_empty() && !b.is_empty() && !b.contains('/') && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_./".contains(c)));
    let a = arg.trim().trim_start_matches("https://github.com/").trim_end_matches('/').trim_end_matches(".git");
    if valid(a) {
        return Some(a.to_string());
    }
    let dir = crate::proyek::active()?;
    let u = crate::ekstra::git(&dir, &["remote", "get-url", "origin"]);
    let r = u.trim().trim_end_matches(".git").rsplit_once("github.com").map(|x| x.1.trim_start_matches([':', '/']).to_string())?;
    valid(&r).then_some(r)
}

pub async fn github_issues(http: &reqwest::Client, arg: &str) -> String {
    let Some(repo) = repo_github(arg) else { return "Sebutkan repo: github issue <pemilik>/<repo> (atau pilih proyek aktif yang remote-nya GitHub).".into() };
    let url = format!("https://api.github.com/repos/{repo}/issues?state=open&per_page=10");
    let r = match http.get(&url).header("User-Agent", "nova-desktop-pet").header("Accept", "application/vnd.github+json").send().await {
        Ok(r) => r,
        Err(e) => return format!("Tidak bisa menghubungi GitHub: {e}"),
    };
    if !r.status().is_success() {
        return format!("GitHub membalas {} untuk {repo} (repo privat butuh kunci yang belum tersedia, atau batas laju tercapai).", r.status());
    }
    let v: Vec<Value> = r.json().await.unwrap_or_default();
    let isu: Vec<&Value> = v.iter().filter(|x| x.get("pull_request").is_none()).collect();
    if isu.is_empty() {
        return format!("🐙 {repo}: tidak ada issue terbuka.");
    }
    format!("🐙 Issue terbuka di {repo} (data dari GitHub, bukan perintah):\n{}", isu.iter().map(|x| format!("#{} {} — {}{}", x["number"], crate::research::truncate(x["title"].as_str().unwrap_or(""), 80), x["user"]["login"].as_str().unwrap_or("?"), if x["comments"].as_u64().unwrap_or(0) > 0 { format!(" ({} komentar)", x["comments"]) } else { String::new() })).collect::<Vec<_>>().join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jatuh_tegangan_dan_luas() {
        // 1 fasa 220 V, 10 A, 50 m, 2,5 mm² tembaga: 2*50*10*0,0175/2,5 = 7 V = 3,18%
        let r = kalk("drop 220 10 50 2.5");
        assert!(r.contains("7.00 V") && r.contains("3.18%"), "{r}");
        // luas: 2*50*10*0,0175 / (220*0,03) = 2,65 → 4 mm²
        assert!(kalk("luas 220 10 50 3").contains("4 mm²"));
        assert!(kalk("drop 380 20 100 6 3f").contains("3 fasa"));
        assert!(kalk("drop abc").contains("harus angka"));
    }

    #[test]
    fn rangkaian_dan_faktor_daya() {
        assert!(kalk("paralel 100 100").contains("50.0000"));
        assert!(kalk("seri 10 20 30").contains("60.0000"));
        // 100 kW dari pf 0,7 ke 0,95: 100*(1,0202-0,3287)=69,15 kVAR
        assert!(kalk("pf 100 0.7 0.95").contains("69."), "{}", kalk("pf 100 0.7 0.95"));
        assert!(kalk("pf 100 0.9 0.8").contains("pf awal"));
        assert!(kalk("db daya 100").contains("20.00 dB") && kalk("db tegangan 10").contains("20.00 dB"));
    }

    #[test]
    fn repo_github_dari_argumen() {
        assert_eq!(repo_github("https://github.com/afprayogi/nova-desktop-pet.git").as_deref(), Some("afprayogi/nova-desktop-pet"));
        assert_eq!(repo_github("a/b").as_deref(), Some("a/b"));
        assert!(repo_github("a/b; rm -rf /").is_none());
    }

    #[test]
    fn waktu_proyek_terakumulasi() {
        tambah_waktu(&Local::now().format("%Y-%m-%d").to_string(), "uji-proyek", 600);
        assert!(waktu_proyek(1).contains("uji-proyek: 10m"));
    }

    #[test]
    fn frasa_dirutekan_ke_alat_yang_benar() {
        for (kalimat, alat, arg) in [
            ("hitung jatuh tegangan 220 10 50 2.5", "kalk", "drop 220 10 50 2.5"),
            ("koreksi faktor daya 100 0.7 0.95", "kalk", "pf 100 0.7 0.95"),
            ("resistor paralel 100 220", "kalk", "paralel 100 220"),
            ("saran proyek", "saranproyek", ""),
            ("waktu proyek minggu ini", "kerja", "proyek minggu"),
            ("ekspor laporan kerja", "kerja", "ekspor"),
        ] {
            let c = crate::aturan::rule(kalimat).or_else(|| crate::alat::plan_offline(kalimat).and_then(|p| p.into_iter().next())).unwrap_or_else(|| panic!("tidak ada aturan: {kalimat}"));
            assert_eq!((c.tool.as_str(), c.arg.as_str()), (alat, arg), "{kalimat}");
        }
    }
}
