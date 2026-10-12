//! Aturan bahasa alami → panggilan alat (tanpa model). Urutan `if` di `rule` menentukan hasil; dijaga oleh tes `rules_golden` di alat.rs.
use crate::alat::Call;

/// Nama umum aplikasi -> perintah yang bisa dibuka Windows.
fn app_alias(a: &str) -> String {
    match a.to_lowercase().trim() {
        "vs code" | "vscode" | "visual studio code" => "code",
        "kalkulator" | "calculator" => "calc",
        "catatan" | "notepad" => "notepad",
        "file explorer" | "explorer" | "penjelajah file" => "explorer",
        "edge" | "microsoft edge" => "msedge",
        "terminal" | "command prompt" => "cmd",
        "paint" => "mspaint",
        _ => return a.to_string(),
    }
    .to_string()
}

/// Sisa teks setelah kata kunci di awal (kata kunci harus utuh).
fn after<'a>(s: &'a str, l: &str, kws: &[&str]) -> Option<&'a str> {
    kws.iter().find_map(|k| {
        (l.starts_with(k) && (l.len() == k.len() || l.as_bytes()[k.len()] == b' ')).then(|| s[k.len()..].trim())
    })
}

fn clean(a: &str) -> String {
    let mut a = a.trim();
    loop {
        let l = a.to_ascii_lowercase();
        match ["di ", "dari ", "pada ", "folder ", "file ", "direktori ", "perintah ", "pdf ", "dokumen "].iter().find(|w| l.starts_with(**w)) {
            Some(w) => a = a[w.len()..].trim_start(),
            None => break,
        }
    }
    a.trim_matches(|c| c == '"' || c == '\'').to_string()
}

/// Buang kata pembuka yang tidak berarti ("halo nova coba tolong ...") agar aturan awalan tetap cocok.
fn strip_fillers(s: &str) -> &str {
    const F: &[&str] = &["halo", "hai", "hi", "hey", "nova", "gemini", "coba", "tolong", "bisa", "bisakah", "kah", "dong", "please", "ya", "yuk", "saya", "aku", "mau", "minta"];
    let mut cur = s.trim();
    loop {
        let first = cur.split_whitespace().next().unwrap_or("");
        let w = first.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
        if !w.is_empty() && F.contains(&w.as_str()) && cur.split_whitespace().count() > 2 {
            cur = cur[first.len()..].trim_start();
        } else {
            return cur;
        }
    }
}

/// Layanan web: kalimat → URL pencarian (YouTube, Maps, Google, GitHub). Dibuka di browser lewat alat `open`.
fn layanan(l: &str) -> Option<String> {
    const NOISE: &[&str] = &["halo", "hai", "gemini", "nova", "coba", "tolong", "buka", "bukakan", "putar", "putarkan", "tonton", "cari", "carikan", "di", "dari", "ke", "video", "youtube", "yt", "google", "googling", "github", "maps", "peta", "rute", "navigasi", "lokasi", "repo", "repositori", "maksudnya", "maksud", "dong", "ya", "dan", "aja", "saja", "pakai", "lewat", "link", "tautan"];
    let w: Vec<&str> = l.split(|c: char| !c.is_alphanumeric()).filter(|x| !x.is_empty()).collect();
    let has = |k: &str| w.contains(&k);
    let q = |extra: &[&str]| w.iter().filter(|x| !NOISE.contains(x) && !extra.contains(x)).cloned().collect::<Vec<_>>().join(" ");
    let files = ["file", "berkas", "folder", "laptop", "mp4", "mkv", "avi", "drive", "download", "unduhan", "komputer"].iter().any(|k| has(k));
    let extra = ["lagu", "musik", "mainkan", "setel", "play"];
    let lagu = (has("lagu") || has("musik")) && ["putar", "putarkan", "mainkan", "setel", "play"].iter().any(|k| has(k)) && !has("folder") && !has("laptop") && !q(&extra).is_empty();
    let (base, q) = if lagu {
        ("https://www.youtube.com/results?search_query=", format!("lagu {}", q(&extra)))
    } else if has("youtube") || has("yt") || (has("video") && ["tonton", "putar", "putarkan", "cari", "carikan", "buka", "bukakan"].iter().any(|k| has(k)) && !files) {
        ("https://www.youtube.com/results?search_query=", q(&[]))
    } else if l.starts_with("rute ke ") || l.starts_with("navigasi ke ") || has("maps") || l.starts_with("peta ") || l.starts_with("lokasi ") {
        ("https://www.google.com/maps/search/", q(&[]))
    } else if has("github") && (has("cari") || has("carikan") || has("repo") || has("repositori")) {
        ("https://github.com/search?type=repositories&q=", q(&[]))
    } else if has("googling") || l.contains("di google") || l.contains("cari google") || l.starts_with("google ") {
        ("https://www.google.com/search?q=", q(&[]))
    } else {
        return None;
    };
    (!q.is_empty()).then(|| format!("{base}{}", urlencoding::encode(&q)))
}

pub fn rule(s: &str) -> Option<Call> {
    let s = strip_fillers(s);
    let l = s.to_ascii_lowercase();
    if let Some(r) = after(s, &l, &["kirim ke email saya", "emailkan ke saya", "kirim email ke saya"]) {
        return Some(Call { tool: "kirim".into(), arg: r.to_string() });
    }
    if let Some(r) = after(s, &l, &["baca dengan chrome", "baca pakai chrome", "buka dan baca"]) {
        return Some(Call { tool: "peramban".into(), arg: clean(r) });
    }
    if let Some(a) = crate::daya::dari_kalimat(&l) {
        return Some(Call { tool: "daya".into(), arg: a.into() });
    }
    if let Some(u) = layanan(&l) {
        return Some(Call { tool: "open".into(), arg: u });
    }
    if let Some((tool, arg)) = crate::perkakas::rule(s, &l) {
        return Some(Call { tool: tool.into(), arg });
    }
    if (l.contains("tidur sekarang") || l.contains("aku tidur") || l.contains("saya tidur")) && ["rencana", "jadwal", "besok", "bagaimana", "gimana", "jadi"].iter().any(|k| l.contains(k)) {
        return Some(Call { tool: "jadwal".into(), arg: "sekarang".into() });
    }
    if ["jurnal", "paper", "makalah ilmiah", "artikel ilmiah", "publikasi ilmiah"].iter().any(|k| l.contains(k)) && (l.contains("cari") || l.contains("temukan") || l.contains("tampilkan")) {
        return Some(Call { tool: "jurnal".into(), arg: s.to_string() });
    }
    // YouTube: "buka youtube dan carikan video anime" -> hasil pencarian YouTube di browser
    if l.contains("youtube") && (l.contains("cari") || l.contains("video") || l.contains("tonton")) && !l.contains("laptop") {
        let q = ["video ", "cari ", "carikan ", "mencari "].iter().filter_map(|k| l.rfind(k).map(|i| s[i + k.len()..].trim().to_string())).next().unwrap_or_default();
        let q = q.replace("di youtube", "").replace("youtube", "").trim().to_string();
        let url = if q.is_empty() { "https://www.youtube.com".to_string() } else { format!("https://www.youtube.com/results?search_query={}", urlencoding::encode(&q)) };
        return Some(Call { tool: "open".into(), arg: url });
    }
    // gambar/foto dari internet
    if (l.contains("internet") || l.contains("online") || l.contains("web")) && (l.contains("gambar") || l.contains("foto") || l.contains("photo")) {
        let q = ["gambar ", "foto ", "photo "].iter().filter_map(|k| l.rfind(k).map(|i| s[i + k.len()..].trim().to_string())).next().unwrap_or_default();
        let q = q.replace("di internet", "").replace("di web", "").replace("internet", "").replace("online", "").trim().to_string();
        if !q.is_empty() {
            return Some(Call { tool: "gambar".into(), arg: q });
        }
    }
    let mk = |t: &str, a: String| Some(Call { tool: t.into(), arg: a });
    for pre in ["carikan gambar ", "cari gambar ", "tampilkan gambar ", "tunjukkan gambar ", "carikan foto ", "cari foto ", "gambar "] {
        if l.starts_with(pre) && l.len() > pre.len() {
            return mk("gambar", s[pre.len()..].trim().to_string());
        }
    }
    if let Some(u) = s.split_whitespace().find(|w| w.starts_with("http://") || w.starts_with("https://")) {
        let t = if l.contains("buka") || l.contains("open") { "open" } else { "fetch" };
        return mk(t, u.to_string());
    }
    if l.contains("layar") && ["baca", "ocr", "ringkas", "rangkum", "tangkap", "isi layar", "apa yang"].iter().any(|k| l.starts_with(k) || l.contains(&format!("{k} ")) ) {
        let ringkas = l.starts_with("ringkas") || l.starts_with("rangkum");
        return mk("layar", if ringkas { "ringkas".into() } else { String::new() });
    }
    if let Some(r) = after(s, &l, &["buatkan kurikulum", "buatkan jadwal belajar", "buat kurikulum", "ini bukunya", "bukunya", "kurikulum", "jadwal belajar", "rencana belajar"]) {
        return mk("kurikulum", r.to_string());
    }
    if let Some(r) = after(s, &l, &["graphify", "petakan folder", "petakan proyek", "petakan"]) {
        return mk("graphify", clean(r));
    }
    if l.contains("kita bahas") || l.starts_with("ingat apa") || l.contains("ingatan tentang") || l.contains("apa yang kamu ingat") || l.contains("pernah kita") {
        let topic = l.split("tentang").nth(1).map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).unwrap_or_else(|| s.to_string());
        return mk("ingatan", topic);
    }
    if l.contains("email") && (l.contains("baru") || l.starts_with("cek") || l.starts_with("lihat") || l.contains("belum dibaca") || l == "email") {
        return mk("email", String::new());
    }
    if let Some(r) = after(s, &l, &["berita", "kabar terbaru"]) {
        return mk("berita", r.to_string());
    }
    if l.contains("kepribadian") || l.contains("preferensimu") {
        return mk("kepribadian", String::new());
    }
    // Perintah bahasa sehari-hari: tutup aplikasi menganggur / rapikan / cari file / meme
    if (l.contains("aplikasi") || l.contains("program") || l.contains("app")) && ["tidak dipakai", "tidak digunakan", "gak dipakai", "nggak dipakai", "nganggur", "tak terpakai", "tidak terpakai", "gak kepake", "ga kepake"].iter().any(|k| l.contains(k)) && ["matikan", "tutup", "bersihkan", "stop"].iter().any(|k| l.contains(k)) || l.contains("bebaskan memori") || l.contains("bebaskan ram") {
        return mk("apps", String::new());
    }
    if l == "urungkan" || l.contains("urungkan rapikan") || l.contains("batalkan rapikan") {
        return mk("urungkan", String::new());
    }
    if let Some(r) = after(s, &l, &["rapikan", "rapihkan", "rapihin", "bereskan", "beresin", "susun file", "bersihkan folder"]) {
        return mk("rapikan", r.to_string());
    }
    if ["file", "berkas", "dokumen", "foto", "gambar", "pdf"].iter().any(|k| l.contains(k)) && (["carikan", "cari", "temukan"].iter().any(|k| l.starts_with(k) || l.contains(&format!(" {k} "))) || l.contains("di mana") || l.contains("dimana")) {
        return mk("cari", s.to_string());
    }
    if ["jam berapa", "pukul berapa", "hari apa", "tanggal berapa", "tanggal sekarang", "hari ini tanggal", "sekarang jam"].iter().any(|k| l.contains(k)) {
        return mk("waktu", String::new());
    }
    if let Some(r) = after(s, &l, &["bukapath"]) {
        return mk("bukapath", r.to_string());
    }
    if l.contains("otomatis") && (l.contains("kecerahan") || l.contains("suara") || l.contains("volume")) {
        let kind = if l.contains("kecerahan") { "kecerahan" } else { "suara" };
        let on = !["matikan", "nonaktif", "mati ", " off", "berhenti", "jangan"].iter().any(|k| l.contains(k)) && !l.ends_with("mati");
        return mk("otomatis", format!("{kind} {}", if on { "on" } else { "off" }));
    }
    if ["bisa tidur", "cukup tidur", "rencana tidur", "jadwal tidur", "atur jadwal", "jadwal besok", "jadwalku", "tidur 8 jam", "tidur delapan jam"].iter().any(|k| l.contains(k)) {
        return mk("jadwal", String::new());
    }
    if l.contains("wifi") || l.contains("wi-fi") {
        if ["cek", "lihat", "apa aja", "apa saja", "daftar", "tampilkan", "tulis", "list", "tersedia", "sekitar"].iter().any(|k| l.contains(k)) {
            return mk("wifi", String::new());
        }
    }
    if l.contains("port") && ["cek", "lihat", "aktif", "listening", "terpakai", "tampilkan", "tulis", "daftar"].iter().any(|k| l.contains(k)) && !l.contains("laporan") && !l.contains("export") {
        return mk("port", String::new());
    }
    if l.contains("bersihkan cache") || l.contains("bersihkan disk") || l.contains("bersihkan penyimpanan") || l.contains("kosongkan cache") {
        return mk("bersih", String::new());
    }
    // "buka vs code folder X" / "buka vscode X" -> buka folder proyek di VS Code
    for pre in ["buka vs code folder ", "buka vscode folder ", "buka visual studio code folder ", "buka vs code proyek ", "buka vs code project ", "buka vs code ", "buka vscode ", "buka code "] {
        if l.starts_with(pre) && l.len() > pre.len() {
            let nama = s[pre.len()..].trim();
            if !nama.is_empty() && !nama.contains(':') && !nama.contains("\\") &&!nama.contains('/') {
                return mk("kode", nama.to_string());
            }
        }
    }
    if let Some(r) = after(s, &l, &["buka file", "bukakan file", "buka berkas", "buka dokumen", "open file", "buka pdf", "bukakan"]) {
        if !r.is_empty() && !r.contains('\\') && !r.contains('/') && !r.contains(':') {
            return mk("bukafile", r.to_string());
        }
    }
    if let Some(r) = after(s, &l, &["buka"]) {
        let last = r.rsplit('.').next().unwrap_or("");
        if r.contains('.') && !r.contains(' ') && (2..=5).contains(&last.len()) && last.chars().all(|c| c.is_ascii_alphanumeric()) && !r.contains('\\') && !r.contains('/') && !r.contains(':') && !r.starts_with("http") && !r.starts_with("www") {
            return mk("bukafile", r.to_string());
        }
    }
    if let Some(r) = after(s, &l, &["bantu", "bantuan", "cmd?", "cli", "help", "cara pakai", "perintah"]) {
        if r.split_whitespace().next().is_some_and(crate::cli::is_tool) {
            return mk("cli", r.to_string());
        }
    }
    if let Some(a) = crate::sistem::parse(&l) {
        return mk("atur", a);
    }
    if let Some(r) = after(s, &l, &["cari di internet", "cari di web", "cari di google", "googling", "cari informasi tentang", "cari info tentang", "browsing"]) {
        return mk("web", r.to_string());
    }
    if let Some(r) = after(s, &l, &["buka", "jalankan", "open"]) {
        if crate::sistem::winapp_match(r).is_some() {
            return mk("winapp", r.to_string());
        }
    }
    if let Some(r) = after(s, &l, &["tugas utama", "tugas hari ini", "fokus hari ini", "prioritas hari ini"]) {
        return mk("tugas", r.to_string());
    }
    if (l.contains("musik") || l.contains("music") || l.contains("lagu")) && ["putar", "nyalakan", "mainkan", "setel", "hidupkan", "play"].iter().any(|k| l.contains(k)) {
        return mk("musik", String::new());
    }
    if let Some(r) = after(s, &l, &["buka folder", "buka hasil", "tunjukkan folder", "folder ke", "buka lokasi"]) {
        if r.chars().any(|c| c.is_ascii_digit()) || r.is_empty() {
            return mk("bukahasil", r.to_string());
        }
    }
    if let Some(r) = after(s, &l, &["meme", "kasih meme", "hibur aku", "lawak"]) {
        return mk("meme", r.to_string());
    }
    if let Some(r) = after(s, &l, &["tulis", "write"]) {
        let (p, body) = match r.to_ascii_lowercase().find(" isi ") {
            Some(i) => (&r[..i], &r[i + 5..]),
            None => (r, ""),
        };
        return mk("write", format!("{}
{}", clean(p), body));
    }
    if let Some(r) = after(s, &l, &["jalankan perintah", "jalankan", "shell", "run", "cmd"]) {
        return mk("shell", clean(r));
    }
    if let Some(r) = after(s, &l, &["isi folder", "isi direktori", "daftar file", "lihat folder", "tampilkan folder", "list", "ls", "dir"]) {
        let p = clean(r);
        return mk("ls", if p.is_empty() { ".".into() } else { p });
    }
    if let Some(r) = after(s, &l, &["ringkas", "rangkum", "summarize", "summary"]) {
        let p = clean(r);
        if !p.is_empty() {
            return mk("ringkas", p);
        }
    }
    if let Some(r) = after(s, &l, &["tampilkan isi file", "baca file", "baca", "read", "cat"]) {
        return mk("read", clean(r));
    }
    let words: Vec<&str> = l.split(|c: char| !c.is_alphanumeric()).collect();
    if let Some(r) = after(s, &l, &["ingatkan saya", "ingatkan", "remind", "pengingat"]) {
        return mk("remind", r.to_string());
    }
    if let Some(r) = after(s, &l, &["pelajari", "serap"]) {
        return mk("pelajari", clean(r));
    }
    if l == "agenda" || l.contains("jadwal hari ini") || l == "jadwal" {
        return mk("agenda", String::new());
    }
    if l.contains("kebiasaan") || l == "habit" || l.contains("statistik pemakaian") {
        return mk("habit", String::new());
    }
    if words.contains(&"tidur") && !words.contains(&"hitung") {
        return mk("tidur", String::new());
    }
    if words.first() == Some(&"tanya") || words.first() == Some(&"hitung") {
        return mk("tanya", s.to_string());
    }
    if words.contains(&"baterai") || (l.contains("daya") && l.contains("laptop")) {
        return mk("baterai", String::new());
    }
    if ["kesehatan", "cpu", "ram", "memori", "disk", "performa", "lambat"].iter().any(|k| words.contains(k)) {
        return mk("health", String::new());
    }
    if ["proses", "process", "tasklist"].iter().any(|k| l.contains(k)) {
        return mk("ps", String::new());
    }
    if l == "info" || ["info sistem", "spesifikasi", "sistem operasi"].iter().any(|k| l.contains(k)) {
        return mk("info", String::new());
    }
    {
        // Hanya path ke buku (pdf/md/txt): otomatis dibuatkan kurikulum.
        let t = s.trim().trim_matches('"');
        let lt = t.to_lowercase();
        if [".pdf", ".md", ".txt"].iter().any(|e| lt.ends_with(e)) && (t.contains('/') || t.contains('\\')) && t.split_whitespace().count() <= 6 {
            return mk("kurikulum", format!("\"{t}\""));
        }
    }
    after(s, &l, &["buka aplikasi", "buka file", "buka", "open", "start"]).map(clean).filter(|a| !a.is_empty()).and_then(|a| mk("open", app_alias(&a)))
}

