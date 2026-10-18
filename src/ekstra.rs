//! Perkakas tambahan: ringkasan git, ringkasan halaman web dari URL, dan analisis cepat berkas CSV.
use std::path::PathBuf;
use std::process::Command;

fn git(dir: &PathBuf, args: &[&str]) -> String {
    let mut c = Command::new("git");
    c.args(args).current_dir(dir);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    c.output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default()
}

/// Ringkasan repo: cabang, perubahan, 5 commit terakhir, statistik diff, dan saran pesan commit (bila model tersedia).
pub async fn git_ringkas(http: &reqwest::Client, arg: &str) -> String {
    let dir = if arg.trim().is_empty() { crate::proyek::active() } else { Some(PathBuf::from(arg.trim().trim_matches('"'))) };
    let Some(dir) = dir.filter(|d| d.is_dir()) else { return "Tentukan folder repo: gitringkas <folder> (atau pilih proyek aktif dengan /proyek <folder>).".into() };
    let status = git(&dir, &["status", "-sb"]);
    if status.is_empty() {
        return format!("{} bukan repositori git (atau git tidak terpasang).", dir.display());
    }
    let log = git(&dir, &["log", "-5", "--pretty=format:%h %s (%cr)"]);
    let stat = git(&dir, &["diff", "--stat", "HEAD"]);
    let n_changed = status.lines().skip(1).count();
    let mut out = format!("🌿 {}\n{}\n\nCommit terakhir:\n{}\n", dir.display(), status.lines().take(12).collect::<Vec<_>>().join("\n"), log);
    if !stat.is_empty() {
        out.push_str(&format!("\nDiff vs HEAD:\n{}\n", stat.lines().rev().take(8).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n")));
    }
    if n_changed > 0 && (crate::gemini::available() || crate::lokal::has(http, crate::lokal::CHAT_MODEL).await) {
        let diff: String = git(&dir, &["diff", "HEAD", "--unified=0"]).chars().take(3500).collect();
        let sys = "Kamu VOLT, ahli pemrograman. Dari diff git berikut: (1) jelaskan perubahan utamanya dalam 2-3 kalimat Bahasa Indonesia, (2) usulkan SATU pesan commit gaya conventional commits. Format: 'Ringkasan: ...' lalu 'Commit: ...'.";
        let r = if crate::gemini::available() { crate::gemini::generate(http, sys, &diff, false, false).await.ok() } else { None };
        let r = match r {
            Some(t) => Some(t),
            None => crate::lokal::chat(http, &format!("{sys}\n\n{diff}")).await.ok(),
        };
        if let Some(t) = r {
            out.push_str(&format!("\n🧠 {t}"));
        }
    }
    out
}

fn strip_html(h: &str) -> String {
    let mut s = h.to_string();
    for tag in ["script", "style", "noscript", "svg", "head"] {
        while let Some(a) = s.to_lowercase().find(&format!("<{tag}")) {
            let end = s.to_lowercase()[a..].find(&format!("</{tag}>")).map(|e| a + e + tag.len() + 3).unwrap_or(s.len());
            s.replace_range(a..end.min(s.len()), " ");
        }
    }
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&nbsp;", " ").replace("&amp;", "&").replace("&quot;", "\"").replace("&#39;", "'").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Ambil halaman web lalu ringkas (model bila ada, jika tidak kalimat-kalimat awal).
pub async fn halaman(http: &reqwest::Client, url: &str) -> String {
    let url = url.split_whitespace().find(|w| w.starts_with("http")).unwrap_or(url.trim());
    if !url.starts_with("http") {
        return "Beri URL lengkap, mis. ringkas halaman https://contoh.com/artikel".into();
    }
    let body = match crate::alat::ambil_publik(url).await {
        Ok(t) => t,
        Err(e) => return format!("Gagal membuka halaman: {e}"),
    };
    let title = body.split("<title>").nth(1).and_then(|t| t.split("</title>").next()).unwrap_or("").trim().to_string();
    let text: String = strip_html(&body).chars().take(6000).collect();
    if text.len() < 80 {
        return "Halaman hampir kosong (mungkin dimuat lewat JavaScript atau butuh login).".into();
    }
    let sys = "Ringkas isi halaman web berikut dalam Bahasa Indonesia: 4-6 poin utama bernomor, lalu satu kalimat kesimpulan. Jangan menambah fakta yang tidak ada di teks.";
    let r = if crate::gemini::available() { crate::gemini::generate(http, sys, &text, false, false).await.ok() } else { None };
    let r = match r {
        Some(t) => Some(t),
        None if crate::lokal::has(http, crate::lokal::CHAT_MODEL).await => crate::lokal::chat(http, &format!("{sys}\n\n{}", text.chars().take(3000).collect::<String>())).await.ok(),
        None => None,
    };
    match r {
        Some(t) => format!("📄 {title}\n{t}\n{url}"),
        None => format!("📄 {title}\n{}…\n(ringkasan model tidak tersedia; ini cuplikan awal)\n{url}", text.chars().take(700).collect::<String>()),
    }
}

fn col_index(r: &str) -> usize {
    r.chars().take_while(|c| c.is_ascii_alphabetic()).fold(0usize, |a, c| a * 26 + (c.to_ascii_uppercase() as usize - 64)).saturating_sub(1)
}

fn xml_text(s: &str) -> String {
    s.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'")
}

/// Baca lembar pertama berkas .xlsx (zip + XML) menjadi tabel teks: (header, baris).
pub fn xlsx_rows(path: &str) -> Result<(Vec<String>, Vec<Vec<String>>), String> {
    use std::io::Read;
    let f = std::fs::File::open(path).map_err(|e| format!("Tidak bisa membuka '{path}': {e}"))?;
    let mut z = zip::ZipArchive::new(f).map_err(|e| format!("Bukan berkas xlsx yang valid: {e}"))?;
    let mut read = |name: &str| -> Option<String> {
        let mut s = String::new();
        z.by_name(name).ok()?.take(20_000_000).read_to_string(&mut s).ok()?;
        Some(s)
    };
    let shared: Vec<String> = read("xl/sharedStrings.xml").map(|x| x.split("<si>").skip(1).map(|si| xml_text(&si.split("<t").skip(1).filter_map(|t| t.split_once('>').and_then(|(_, r)| r.split("</t>").next())).collect::<Vec<_>>().join(""))).collect()).unwrap_or_default();
    let sheet = read("xl/worksheets/sheet1.xml").ok_or("lembar pertama (sheet1) tidak ditemukan")?;
    let mut rows: Vec<Vec<String>> = vec![];
    for row in sheet.split("<row ").skip(1) {
        let row = row.split("</row>").next().unwrap_or("");
        let mut cells: Vec<String> = vec![];
        for c in row.split("<c ").skip(1) {
            let attrs = c.split('>').next().unwrap_or("");
            let rf = attrs.split("r=\"").nth(1).and_then(|x| x.split('"').next()).unwrap_or("A1");
            let t = attrs.split("t=\"").nth(1).and_then(|x| x.split('"').next()).unwrap_or("");
            let v = c.split("<v>").nth(1).and_then(|x| x.split("</v>").next()).unwrap_or("");
            let val = match t {
                "s" => v.parse::<usize>().ok().and_then(|i| shared.get(i).cloned()).unwrap_or_default(),
                "inlineStr" => xml_text(&c.split("<t").nth(1).and_then(|x| x.split_once('>')).and_then(|(_, r)| r.split("</t>").next()).unwrap_or("").to_string()),
                _ => xml_text(v),
            };
            let ix = col_index(rf);
            if cells.len() <= ix {
                cells.resize(ix + 1, String::new());
            }
            cells[ix] = val;
        }
        if cells.iter().any(|c| !c.is_empty()) {
            rows.push(cells);
        }
    }
    if rows.is_empty() {
        return Err("lembar pertama kosong".into());
    }
    let head = rows.remove(0);
    Ok((head, rows))
}

/// Analisis cepat CSV/XLSX: ukuran, kolom, statistik kolom angka, 5 baris pertama, dan grafik kolom angka pertama.
pub fn csv(arg: &str) -> String {
    let path = arg.split_whitespace().find(|w| { let l = w.to_lowercase(); l.ends_with(".csv") || l.ends_with(".xlsx") }).map(|w| w.trim_matches('"').to_string()).unwrap_or_else(|| arg.trim().trim_matches('"').to_string());
    let (cols, rows, delim): (Vec<String>, Vec<Vec<String>>, char) = if path.to_lowercase().ends_with(".xlsx") {
        match xlsx_rows(&path) {
            Ok((c, r)) => (c, r, '|'),
            Err(e) => return e,
        }
    } else {
        let Ok(text) = std::fs::read_to_string(&path) else { return format!("Tidak bisa membaca '{path}'. Beri jalur lengkap berkas .csv atau .xlsx.") };
        let mut lines = text.lines().filter(|l| !l.trim().is_empty());
        let Some(head) = lines.next() else { return "Berkas kosong.".into() };
        let delim = [';', ',', '\t'].into_iter().max_by_key(|d| head.matches(*d).count()).unwrap_or(',');
        (head.split(delim).map(|c| c.trim().trim_matches('"').to_string()).collect(), lines.map(|l| l.split(delim).map(|c| c.trim().trim_matches('"').to_string()).collect()).collect(), delim)
    };
    let mut out = format!("📊 {} — {} baris × {} kolom{}\n", path, rows.len(), cols.len(), if path.to_lowercase().ends_with(".xlsx") { " (lembar pertama)".to_string() } else { format!(" (pemisah '{}')", if delim == '\t' { "tab".to_string() } else { delim.to_string() }) });
    let mut chart: Option<String> = None;
    for (i, c) in cols.iter().enumerate().take(12) {
        let vals: Vec<f64> = rows.iter().filter_map(|r| r.get(i)).filter_map(|v| v.replace(',', ".").parse::<f64>().ok()).collect();
        let kosong = rows.iter().filter(|r| r.get(i).map_or(true, |v| v.is_empty())).count();
        if vals.len() * 2 >= rows.len() && !vals.is_empty() {
            let (mn, mx) = vals.iter().fold((f64::MAX, f64::MIN), |a, v| (a.0.min(*v), a.1.max(*v)));
            out.push_str(&format!("• {c}: angka · min {mn} · maks {mx} · rata-rata {:.3} · jumlah {:.3}{}\n", vals.iter().sum::<f64>() / vals.len() as f64, vals.iter().sum::<f64>(), if kosong > 0 { format!(" · kosong {kosong}") } else { String::new() }));
            if chart.is_none() && vals.len() >= 3 {
                chart = Some(format!("CHART {}", serde_json::json!({"t": format!("{c} (kolom angka pertama)"), "u": "", "v": vals.iter().take(80).collect::<Vec<_>>()})));
            }
        } else {
            let mut u: Vec<&String> = rows.iter().filter_map(|r| r.get(i)).collect();
            u.sort();
            u.dedup();
            out.push_str(&format!("• {c}: teks · {} nilai unik{}\n", u.len(), if kosong > 0 { format!(" · kosong {kosong}") } else { String::new() }));
        }
    }
    if let Some(c) = chart {
        out.push_str(&format!("{c}\n"));
    }
    out.push_str("\n5 baris pertama:\n");
    for r in rows.iter().take(5) {
        out.push_str(&format!("{}\n", r.iter().take(8).cloned().collect::<Vec<_>>().join(" | ")));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn csv_stats_and_html_strip() {
        let f = std::env::temp_dir().join(format!("nova_csv_{}.csv", std::process::id()));
        std::fs::write(&f, "nama;nilai\nA;10\nB;20\nC;30\n").unwrap();
        let r = csv(&f.to_string_lossy());
        assert!(r.contains("3 baris × 2 kolom") && r.contains("rata-rata 20.000"));
        let _ = std::fs::remove_file(&f);
        assert_eq!(strip_html("<p>Halo <b>dunia</b></p><script>x()</script>"), "Halo dunia");
    }
}
