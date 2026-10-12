//! Alat tambahan yang meniru perkakas umum CrewAI (cari-isi-berkas, pencarian GitHub, tunggu, kompres) tanpa kunci API.
use std::io::Write;

/// `<berkas> :: <pertanyaan>` → potongan paling relevan dari berkas (txt/md/csv/json/pdf/docx lewat `alat::read_doc`).
pub fn cari_isi(arg: &str) -> String {
    let Some((path, q)) = arg.split_once("::").or_else(|| arg.split_once(" | ")) else {
        return "Format: cariisi <berkas> :: <pertanyaan>".into();
    };
    let (path, q) = (path.trim().trim_matches('"'), q.trim());
    if q.is_empty() {
        return "Tulis pertanyaannya setelah '::'.".into();
    }
    let text = match crate::alat::read_doc(path) {
        Ok(t) => t,
        Err(e) => return format!("gagal membaca: {e}"),
    };
    let qv = crate::ingat::embed(q);
    let kata: Vec<String> = q.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| w.len() > 2).map(String::from).collect();
    let mut skor: Vec<(f32, &str)> = crate::teknik::pecah(&text)
        .into_iter()
        .map(|c| {
            let cos: f32 = crate::ingat::embed(c).iter().zip(&qv).map(|(a, b)| a * b).sum();
            let l = c.to_lowercase();
            let lex = kata.iter().filter(|w| l.contains(w.as_str())).count() as f32 / kata.len().max(1) as f32;
            (cos + lex, c)
        })
        .collect();
    skor.sort_by(|a, b| b.0.total_cmp(&a.0));
    let top: Vec<String> = skor.iter().take(3).filter(|(s, _)| *s > 0.15).map(|(_, c)| crate::research::truncate(c.trim(), 400)).collect();
    if top.is_empty() {
        format!("Tidak ada bagian di '{path}' yang cocok dengan '{q}'.")
    } else {
        top.iter().enumerate().map(|(i, t)| format!("[{}] {t}", i + 1)).collect::<Vec<_>>().join("\n---\n")
    }
}

/// Cari repositori GitHub (API publik tanpa kunci): 5 teratas menurut bintang.
pub async fn github(http: &reqwest::Client, q: &str) -> String {
    if q.trim().is_empty() {
        return "Format: github <kata kunci>".into();
    }
    let url = format!("https://api.github.com/search/repositories?q={}&sort=stars&per_page=5", urlencoding::encode(q.trim()));
    let v: serde_json::Value = match http.get(&url).header("User-Agent", "nova-asisten").timeout(std::time::Duration::from_secs(15)).send().await {
        Ok(r) => r.json().await.unwrap_or_default(),
        Err(e) => return format!("GitHub tidak terjangkau: {e}"),
    };
    let rows: Vec<String> = v["items"]
        .as_array()
        .map(|a| a.iter().map(|i| format!("{} ★{} [{}] {} — {}", i["full_name"].as_str().unwrap_or("?"), i["stargazers_count"], i["language"].as_str().unwrap_or("-"), i["license"]["spdx_id"].as_str().unwrap_or("tanpa lisensi"), research_potong(i["description"].as_str().unwrap_or("")))).collect())
        .unwrap_or_default();
    if rows.is_empty() {
        format!("Tidak ada repositori untuk '{q}' (atau batas panggilan GitHub tercapai).")
    } else {
        rows.join("\n")
    }
}

fn research_potong(s: &str) -> String {
    crate::research::truncate(s, 110)
}

pub async fn tunggu(arg: &str) -> String {
    let n: u64 = arg.trim().trim_end_matches(|c: char| c.is_alphabetic() || c.is_whitespace()).parse().unwrap_or(1).clamp(1, 30);
    tokio::time::sleep(std::time::Duration::from_secs(n)).await;
    format!("menunggu {n} detik selesai")
}

fn kumpulkan(dir: &std::path::Path, depth: u32, out: &mut Vec<std::path::PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() && depth < 6 {
            kumpulkan(&p, depth + 1, out);
        } else if p.is_file() && out.len() < 5000 {
            out.push(p);
        }
    }
}

/// Kompres berkas/folder menjadi `<nama>.zip` di sebelahnya (tidak menimpa). Maks 5000 berkas dan 1 GB.
pub fn kompres(arg: &str) -> String {
    let src = std::path::PathBuf::from(arg.trim().trim_matches('"'));
    if !src.exists() {
        return format!("'{}' tidak ditemukan.", src.display());
    }
    let out = std::path::PathBuf::from(format!("{}.zip", src.display()));
    if out.exists() {
        return format!("{} sudah ada; tidak ditimpa.", out.display());
    }
    let mut files = vec![];
    if src.is_dir() {
        kumpulkan(&src, 0, &mut files);
    } else {
        files.push(src.clone());
    }
    let total: u64 = files.iter().filter_map(|f| std::fs::metadata(f).ok()).map(|m| m.len()).sum();
    if total > 1 << 30 {
        return format!("Terlalu besar ({:.1} GB); batas 1 GB.", total as f64 / 1e9);
    }
    let base = if src.is_dir() { src.clone() } else { src.parent().map(|p| p.to_path_buf()).unwrap_or_default() };
    let Ok(f) = std::fs::File::create(&out) else { return format!("Tidak bisa membuat {}.", out.display()) };
    let mut z = zip::ZipWriter::new(f);
    let opt = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut n = 0;
    for p in &files {
        let rel = p.strip_prefix(&base).unwrap_or(p).to_string_lossy().replace('\\', "/");
        if z.start_file(rel, opt).is_ok() {
            if let Ok(b) = std::fs::read(p) {
                if z.write_all(&b).is_ok() {
                    n += 1;
                }
            }
        }
    }
    match z.finish() {
        Ok(_) => format!("{n} berkas dikompres ke {} ({:.1} MB)", out.display(), std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0) as f64 / 1e6),
        Err(e) => format!("gagal menutup zip: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn search_and_zip_roundtrip() {
        let d = std::env::temp_dir().join(format!("nova-alatbaru-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("catatan.txt");
        std::fs::write(&f, format!("{}\n\nKabel NYY 4x16 dipakai untuk panel utama dengan arus 63 ampere.\n\n{}", "Rapat mingguan membahas jadwal.".repeat(30), "Catatan makan siang dan parkir.".repeat(30))).unwrap();
        let r = cari_isi(&format!("{} :: kabel panel utama", f.display()));
        assert!(r.contains("NYY"), "{r}");
        assert!(kompres(&d.display().to_string()).contains("1 berkas"));
        assert!(kompres(&d.display().to_string()).contains("sudah ada"));
        let _ = std::fs::remove_file(format!("{}.zip", d.display()));
        let _ = std::fs::remove_dir_all(&d);
    }
}
