//! Pembuat jadwal belajar / kurikulum dari BUKU (PDF/txt/md) tanpa model: mengenali bab dari judul-judul dokumen
//! (atau membagi rata), memperkirakan waktu baca dari jumlah kata, mengisi hari belajar sesuai anggaran menit harian,
//! menambah ulasan berjarak (+1 dan +7 hari), lalu membuat pengingat. Tanpa buku: kurikulum topik dari graf pengetahuan.
use chrono::{Duration, Local, NaiveDate, TimeZone};

#[derive(Clone, Debug)]
pub struct Section {
    pub title: String,
    pub words: usize,
}

const WPM: f32 = 180.0; // kecepatan baca teknis + mencatat

fn is_toc_line(l: &str) -> bool {
    l.contains("....") || l.trim_end().chars().last().is_some_and(|c| c.is_ascii_digit()) && l.contains("  ")
}

fn heading(l: &str) -> Option<String> {
    let t = l.trim();
    if t.len() < 4 || t.len() > 80 || is_toc_line(t) || t.split_whitespace().count() > 12 {
        return None;
    }
    let low = t.to_lowercase();
    let starts = ["bab ", "chapter ", "bagian ", "part ", "unit ", "modul ", "section "];
    if starts.iter().any(|s| low.starts_with(s)) && t.chars().any(|c| c.is_ascii_digit() || "IVXivx".contains(c)) {
        return Some(t.to_string());
    }
    // "3 Judul" atau "3. Judul" (tingkat atas saja)
    let mut it = t.splitn(2, |c: char| c == ' ' || c == '.' || c == ')');
    let (num, rest) = (it.next()?, it.next()?.trim_start_matches(|c: char| c == ' ' || c == '.'));
    (num.len() <= 2 && !num.is_empty() && num.chars().all(|c| c.is_ascii_digit()) && rest.chars().next().is_some_and(|c| c.is_uppercase()) && !t.ends_with('.')).then(|| t.to_string())
}

pub fn outline(text: &str) -> Vec<Section> {
    let mut secs: Vec<Section> = vec![];
    for line in text.lines() {
        if let Some(h) = heading(line) {
            if secs.last().map(|s: &Section| s.title == h).unwrap_or(false) {
                continue;
            }
            secs.push(Section { title: h, words: 0 });
        } else if let Some(s) = secs.last_mut() {
            s.words += line.split_whitespace().count();
        }
    }
    secs.retain(|s| s.words >= 150); // buang daftar isi/judul kosong
    if secs.len() >= 3 {
        return secs;
    }
    // Cadangan: bagi rata ~3000 kata.
    let words: Vec<&str> = text.split_whitespace().collect();
    let parts = (words.len() as f32 / 3000.0).ceil().max(1.0) as usize;
    (0..parts).map(|i| Section { title: format!("Bagian {} dari {}", i + 1, parts), words: words.len() / parts }).collect()
}

pub fn minutes(s: &Section) -> u32 {
    ((s.words as f32 / WPM * 1.3).ceil() as u32).clamp(10, 600)
}

pub struct Day {
    pub date: NaiveDate,
    pub items: Vec<(String, u32)>,
}

/// Mengisi hari belajar. Mengembalikan (hari, menit/hari efektif).
pub fn schedule(items: &[(String, u32)], horizon: Option<u32>, daily: u32, start: NaiveDate) -> (Vec<Day>, u32) {
    let total: u32 = items.iter().map(|x| x.1).sum();
    let daily = match horizon {
        Some(h) if h > 0 => daily.max((total as f32 / h as f32).ceil() as u32).min(300),
        _ => daily,
    }
    .max(15);
    let mut days: Vec<Day> = vec![Day { date: start, items: vec![] }];
    let mut left = daily;
    let mut finish_day: Vec<(usize, String)> = vec![];
    for (title, mins) in items {
        let mut rest = *mins;
        let mut first = true;
        while rest > 0 {
            let take = rest.min(left);
            let name = if first { title.clone() } else { format!("{title} (lanjutan)") };
            days.last_mut().unwrap().items.push((name, take));
            rest -= take;
            left -= take;
            first = false;
            if left == 0 {
                let d = days.last().unwrap().date + Duration::days(1);
                days.push(Day { date: d, items: vec![] });
                left = daily;
            }
        }
        finish_day.push((days.len() - 1, title.clone()));
    }
    // Ulasan berjarak: +1 dan +7 hari setelah selesai tiap bagian.
    for (d, title) in finish_day {
        for off in [1usize, 7] {
            while days.len() <= d + off {
                let nd = days.last().unwrap().date + Duration::days(1);
                days.push(Day { date: nd, items: vec![] });
            }
            days[d + off].items.push((format!("Ulasan cepat: {title}"), 10));
        }
    }
    days.retain(|d| !d.items.is_empty());
    (days, daily)
}

fn parse_args(arg: &str) -> (String, Option<u32>, Option<u32>) {
    let mut horizon = None;
    let mut daily = None;
    let toks: Vec<String> = arg.split_whitespace().map(|t| t.to_string()).collect();
    let mut path = String::new();
    // jalur berkutip
    if let (Some(a), Some(b)) = (arg.find('"'), arg.rfind('"')) {
        if b > a {
            path = arg[a + 1..b].to_string();
        }
    }
    for (i, t) in toks.iter().enumerate() {
        let low = t.to_lowercase();
        if path.is_empty() && [".pdf", ".txt", ".md"].iter().any(|e| low.trim_matches('"').ends_with(e)) {
            path = t.trim_matches('"').to_string();
        }
        let (num, unit) = {
            let end = low.find(|c: char| !(c.is_ascii_digit())).unwrap_or(low.len());
            if end > 0 && end < low.len() {
                (low[..end].parse::<u32>().ok(), low[end..].to_string())
            } else if end == low.len() && end > 0 {
                (low.parse::<u32>().ok(), toks.get(i + 1).map(|s| s.to_lowercase()).unwrap_or_default())
            } else {
                (None, String::new())
            }
        };
        if let Some(n) = num {
            match unit.as_str() {
                "hari" => horizon = Some(n),
                "minggu" => horizon = Some(n * 7),
                "bulan" => horizon = Some(n * 30),
                "menit" => daily = Some(n),
                "jam" => daily = Some(n * 60),
                _ => {}
            }
        }
    }
    (path, horizon, daily)
}

fn render(title: &str, days: &[Day], daily: u32, total_min: u32) -> String {
    let mut s = format!("📚 Kurikulum: {title}\nTotal ±{} jam {} menit, {} hari belajar, ±{daily} menit/hari (mulai {}).\n", total_min / 60, total_min % 60, days.len(), days.first().map(|d| d.date.format("%d/%m").to_string()).unwrap_or_default());
    for (i, d) in days.iter().enumerate().take(14) {
        s.push_str(&format!("Hari {} ({}): {}\n", i + 1, d.date.format("%a %d/%m"), d.items.iter().map(|(t, m)| format!("{t} [{m}m]")).collect::<Vec<_>>().join("; ")));
    }
    if days.len() > 14 {
        s.push_str(&format!("…dan {} hari lagi.\n", days.len() - 14));
    }
    s
}

fn start_time() -> (u32, u32) {
    let ex = crate::assistant::schedule().explore.0;
    let (h, m) = ex.split_once(':').unwrap_or(("19", "0"));
    let mins = h.parse::<u32>().unwrap_or(19) * 60 + m.parse::<u32>().unwrap_or(0) + 30;
    (mins / 60 % 24, mins % 60)
}

fn make_reminders(days: &[Day], title: &str) -> usize {
    let (h, m) = start_time();
    let now = Local::now();
    let mut n = 0;
    for (i, d) in days.iter().enumerate().take(60) {
        let Some(dt) = d.date.and_hms_opt(h, m, 0).and_then(|x| Local.from_local_datetime(&x).single()) else { continue };
        if dt <= now {
            continue;
        }
        let what: String = d.items.iter().map(|x| x.0.clone()).collect::<Vec<_>>().join(", ").chars().take(120).collect();
        crate::assistant::insert_reminder(dt.timestamp(), &format!("📚 Belajar hari {} ({title}): {what}", i + 1));
        n += 1;
    }
    n
}

/// Folder tempat Bos menaruh buku: Dokumen\NOVA\buku. Buku baru di sini otomatis dipelajari dan dibuatkan kurikulum.
pub fn buku_dir() -> std::path::PathBuf {
    dirs::document_dir().unwrap_or_else(crate::config::data_dir).join("NOVA").join("buku")
}

fn new_books() -> Vec<std::path::PathBuf> {
    let d = buku_dir();
    let _ = std::fs::create_dir_all(&d);
    let Ok(c) = crate::assistant::db() else { return vec![] };
    let _ = c.execute_batch("CREATE TABLE IF NOT EXISTS buku_seen(key TEXT PRIMARY KEY);");
    let mut out = vec![];
    if let Ok(rd) = std::fs::read_dir(&d) {
        for e in rd.flatten() {
            let p = e.path();
            let ok = p.extension().and_then(|x| x.to_str()).is_some_and(|x| ["pdf", "md", "txt"].contains(&x.to_lowercase().as_str()));
            if !ok {
                continue;
            }
            let mt = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
            // "v2": buku yang dulu gagal dibaca (dan terlanjur dianggap sudah dilihat) dicoba lagi dengan pembaca baru.
            let key = format!("{}|{mt}|v2", p.display());
            if c.execute("INSERT OR IGNORE INTO buku_seen VALUES(?1)", [&key]).map(|n| n == 1).unwrap_or(false) {
                out.push(p);
            }
        }
    }
    out
}

/// Dipanggil berkala: buku baru -> pelajari + kurikulum otomatis + pengingat.
pub async fn process_new(tx: &crate::engine::Tx) {
    let books = tokio::task::spawn_blocking(new_books).await.unwrap_or_default();
    for b in books.into_iter().take(2) {
        let name = b.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        tx.sys(format!("📚 Buku baru terdeteksi: {name}. Menyusun kurikulum..."));
        let path = b.to_string_lossy().to_string();
        let plan = tokio::task::spawn_blocking(move || run(&path)).await.unwrap_or_else(|_| "gagal membuat kurikulum".into());
        if plan.starts_with("gagal") {
            // Jangan pura-pura berhasil: beri tahu alasannya dan cara mencoba manual.
            tx.sys(format!("⚠ Buku '{name}' belum bisa dipelajari: {plan}. Coba manual: pelajari <path buku>."));
            continue;
        }
        crate::notify::toast("NOVA - kurikulum baru", &format!("Kurikulum untuk '{name}' sudah siap."));
        crate::adegan::play(tx, vec![(3, format!("📚 Buku baru: {name}. Kurikulumnya sudah kusiapkan:")), (3, plan.chars().take(1800).collect())]);
    }
}

/// Titik masuk alat `kurikulum <path|topik> [N hari|minggu] [N menit|jam]`.
pub fn run(arg: &str) -> String {
    let (path, horizon, daily) = parse_args(arg);
    let daily = daily.unwrap_or(60);
    let start = (Local::now() + Duration::days(1)).date_naive();
    if !path.is_empty() {
        let text = match crate::alat::read_doc_max(&path, 3_000_000) {
            Ok(t) => t,
            Err(e) => return format!("gagal membaca buku: {e}"),
        };
        let secs = outline(&text);
        let items: Vec<(String, u32)> = secs.iter().map(|s| (s.title.clone(), minutes(s))).collect();
        let total: u32 = items.iter().map(|x| x.1).sum();
        let (days, eff) = schedule(&items, horizon, daily, start);
        let title = std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "buku".into());
        let n = make_reminders(&days, &title);
        let learned = crate::teknik::pelajari(&path);
        return format!("{}\n{n} pengingat belajar dibuat (jam {:02}:{:02}). {learned}", render(&title, &days, eff, total), start_time().0, start_time().1);
    }
    // Tanpa buku: kurikulum topik dari graf pengetahuan lokal.
    let topic: String = arg.split_whitespace().filter(|w| !w.chars().any(|c| c.is_ascii_digit()) && !["hari", "minggu", "bulan", "menit", "jam", "belajar", "kurikulum", "jadwal", "buatkan", "selama"].contains(&w.to_lowercase().as_str())).collect::<Vec<_>>().join(" ");
    if topic.trim().is_empty() {
        return "Beri aku bukunya: kurikulum C:/path/buku.pdf selama 3 minggu 45 menit. Atau sebut topik: kurikulum rust 2 minggu.".into();
    }
    let docs = crate::ingat::hybrid(&topic, 2);
    let Some(first) = docs.first() else { return format!("Belum ada bahan tentang '{topic}'. Ajari aku dengan buku: pelajari C:/path/buku.pdf lalu kurikulum buku itu.") };
    let mut lessons = vec![first.title.clone()];
    for t in crate::ingat::related(&first.title, 8) {
        if !lessons.contains(&t) {
            lessons.push(t);
        }
    }
    let items: Vec<(String, u32)> = lessons.iter().map(|t| (t.clone(), 40)).collect();
    let total: u32 = items.iter().map(|x| x.1).sum();
    let (days, eff) = schedule(&items, horizon, daily, start);
    let n = make_reminders(&days, &topic);
    format!("{}\n{n} pengingat dibuat. Tanya isi tiap pelajaran: tanya <judul>. Untuk kurikulum dari buku, beri path PDF-nya.", render(&topic, &days, eff, total))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn chapter(n: usize, words: usize) -> String {
        format!("Bab {n}\n{}\n", "kata ".repeat(words))
    }
    #[test]
    fn outline_and_schedule() {
        let text = format!("DAFTAR ISI\nBab 1 ........ 5\n{}{}{}", chapter(1, 900), chapter(2, 1800), chapter(3, 600));
        let o = outline(&text);
        assert_eq!(o.len(), 3);
        assert_eq!(o[1].title, "Bab 2");
        let items: Vec<(String, u32)> = o.iter().map(|s| (s.title.clone(), minutes(s))).collect();
        let total: u32 = items.iter().map(|x| x.1).sum();
        let (days, eff) = schedule(&items, Some(2), 30, NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
        assert!(eff * 2 >= total, "anggaran harian harus cukup untuk horizon");
        assert!(days.iter().any(|d| d.items.iter().any(|i| i.0.starts_with("Ulasan"))));
    }
    #[test]
    fn args() {
        let (p, h, d) = parse_args("C:/a/buku.pdf selama 3 minggu 45 menit");
        assert_eq!((p.as_str(), h, d), ("C:/a/buku.pdf", Some(21), Some(45)));
        let (_, h2, d2) = parse_args("rust 2 minggu 1 jam");
        assert_eq!((h2, d2), (Some(14), Some(60)));
    }
}
