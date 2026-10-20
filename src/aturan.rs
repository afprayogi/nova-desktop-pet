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
    let mut cur = s.trim();
    loop {
        let first = cur.split_whitespace().next().unwrap_or("");
        let w = first.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
        if !w.is_empty() && crate::lex::ada("pengisi", &w) && cur.split_whitespace().count() > 2 {
            cur = cur[first.len()..].trim_start();
        } else {
            return cur;
        }
    }
}

/// Layanan web bawaan (ditulis ke `layanan.json`; tambah entri lewat berkas atau `/layanan tambah`). Kolom:
/// pemicu (kata), pemicu_awal (awalan kalimat), pemicu_frasa (frasa), butuh_aksi (aksi baku dari kamus aksi), butuh_kata, tolak,
/// buang (kata yang tidak ikut kueri), awalan (disisipkan di depan kueri), url (dengan {q}).
pub const LAYANAN_BAWAAN: &str = r#"{
  "abaikan": ["halo", "hai", "gemini", "nova", "coba", "tolong", "buka", "bukakan", "putar", "putarkan", "tonton", "cari", "carikan", "di", "dari", "ke", "video", "youtube", "yt", "google", "googling", "github", "maps", "peta", "rute", "navigasi", "lokasi", "repo", "repositori", "maksudnya", "maksud", "dong", "ya", "dan", "aja", "kamu", "kau", "anda", "saya", "aku", "mohon", "deh", "sih", "nih", "lah", "bisa", "mau", "ingin", "silakan", "saja", "pakai", "lewat", "link", "tautan", "internet", "online", "situs", "website", "web"],
  "layanan": [
    {"nama": "lagu", "pemicu": ["lagu", "musik", "music"], "butuh_aksi": ["putar", "buka", "ganti"], "tolak": ["folder", "laptop", "spotify"],
     "buang": ["lagu", "musik", "music", "mainkan", "setel", "setelkan", "play", "nyalakan", "dengar", "dengarkan", "ganti", "gantikan", "ubah", "ubahkan", "pindah", "tukar"],
     "awalan": "lagu", "url": "https://www.youtube.com/results?search_query={q}"},
    {"nama": "youtube", "pemicu": ["youtube", "yt"], "url": "https://www.youtube.com/results?search_query={q}"},
    {"nama": "video", "pemicu": ["video"], "butuh_kata": ["tonton", "putar", "putarkan", "cari", "carikan", "buka", "bukakan"], "tolak": ["file", "berkas", "folder", "laptop", "mp4", "mkv", "avi", "drive", "download", "unduhan", "komputer"], "url": "https://www.youtube.com/results?search_query={q}"},
    {"nama": "maps", "pemicu": ["maps"], "pemicu_awal": ["rute ke ", "navigasi ke ", "peta ", "lokasi "], "url": "https://www.google.com/maps/search/{q}"},
    {"nama": "github", "pemicu": ["github"], "butuh_kata": ["cari", "carikan", "repo", "repositori"], "url": "https://github.com/search?type=repositories&q={q}"},
    {"nama": "google", "pemicu": ["googling"], "pemicu_awal": ["google "], "pemicu_frasa": ["di google", "cari google"], "url": "https://www.google.com/search?q={q}"}
  ]
}"#;

pub fn layanan_daftar() -> Vec<String> {
    crate::data::json("layanan.json", LAYANAN_BAWAAN)["layanan"].as_array().map(|a| a.iter().filter_map(|x| x["nama"].as_str().map(String::from)).collect()).unwrap_or_default()
}

/// `/layanan tambah <nama> <pemicu,pemicu> <url dengan {q}>`.
pub fn layanan_tambah(arg: &str) -> String {
    let p: Vec<&str> = arg.split_whitespace().collect();
    if p.len() < 3 || !p[2].contains("{q}") || !(p[2].starts_with("https://") || p[2].starts_with("http://")) {
        return "Pakai: /layanan tambah <nama> <kata1,kata2> <https://situs/cari?q={q}>. Contoh: /layanan tambah wikipedia wiki,wikipedia https://id.wikipedia.org/w/index.php?search={q}".into();
    }
    let (nama, pemicu, url) = (p[0].to_lowercase(), p[1].to_lowercase().split(',').map(String::from).collect::<Vec<_>>(), p[2].to_string());
    let n2 = nama.clone();
    match crate::data::ubah("layanan.json", LAYANAN_BAWAAN, move |v| {
        if let Some(a) = v["layanan"].as_array_mut() {
            a.retain(|x| x["nama"].as_str() != Some(n2.as_str()));
            a.push(serde_json::json!({"nama": n2, "pemicu": pemicu, "url": url}));
        }
    }) {
        Ok(()) => format!("Layanan '{nama}' ditambahkan. Coba: \"cari <sesuatu> di {nama}\"."),
        Err(e) => format!("Gagal menyimpan layanan.json: {e}"),
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Fase {
    /// Pemicu berupa merek/situs (claude, tokopedia, spotify, "buka music ...", youtube): boleh mendahului semua aturan lokal.
    Tinggi,
    /// Pemicu umum (cuaca, kurs, agenda, tips, ...): hanya cadangan setelah aturan lokal dan pemahaman makna tidak mengenali kalimat,
    /// supaya kemampuan lokal NOVA (cuaca, kurs, hitung, agenda, pengingat, buku) tidak terdorong ke pencarian web.
    Rendah,
}

/// Entri bawaan yang pemicunya kata umum tetapi memang harus mendahului (sama seperti perilaku sebelumnya).
const LAYANAN_UTAMA: &[&str] = &["lagu", "youtube", "video", "maps", "github", "google"];

fn host(url: &str) -> String {
    url.split("://").nth(1).unwrap_or(url).split('/').next().unwrap_or("").to_lowercase()
}

/// Kata/frasa pemicu dianggap merek bila salah satu token (≥3 huruf) muncul di nama situs tujuan.
fn merek(pemicu: &str, host: &str) -> bool {
    pemicu.split(|c: char| !c.is_alphanumeric()).any(|t| t.len() >= 3 && host.contains(t))
}

fn pertanyaan_info(l: &str) -> bool {
    let awal = l.split_whitespace().next().unwrap_or("");
    l.trim_end().ends_with('?') || ["apa", "siapa", "bagaimana", "gimana", "kenapa", "mengapa", "jelaskan", "apakah", "berapa", "kapan", "mengapa"].contains(&awal)
}

/// Layanan web: kalimat → URL pencarian, berdasarkan layanan.json. Dibuka di browser lewat alat `open`.
fn layanan(l: &str, fase: Fase) -> Option<String> {
    let cfg = crate::data::json("layanan.json", LAYANAN_BAWAAN);
    let teks = |v: &serde_json::Value| -> Vec<String> { v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_lowercase())).collect()).unwrap_or_default() };
    if fase == Fase::Rendah && pertanyaan_info(l) {
        return None; // pertanyaan pengetahuan dijawab dari otak (buku) atau model, bukan dilempar ke mesin pencari
    }
    let abaikan = teks(&cfg["abaikan"]);
    let w: Vec<&str> = l.split(|c: char| !c.is_alphanumeric()).filter(|x| !x.is_empty()).collect();
    // "cari di internet/web X" tanpa menyebut situs tertentu: jawab di sini lewat alat `web` (dengan sumber), bukan buka Google di browser.
    if ["cari di internet", "cari di web", "cari internet", "cari online", "cari tahu di internet", "cari tahu di web"].iter().any(|k| l.contains(k))
        && !w.iter().any(|x| LAYANAN_UTAMA.contains(x))
    {
        return None;
    }
    let has = |k: &str| w.contains(&k);
    let aksi = crate::kata::aksi(l);
    for e in cfg["layanan"].as_array()? {
        let nama = e["nama"].as_str().unwrap_or("");
        let h = host(e["url"].as_str().unwrap_or(""));
        let utama = LAYANAN_UTAMA.contains(&nama) || e["prioritas"].as_str() == Some("tinggi");
        let (pemicu, awal, frasa) = (teks(&e["pemicu"]), teks(&e["pemicu_awal"]), teks(&e["pemicu_frasa"]));
        let (mut hit_merek, mut hit_umum) = (false, false);
        for k in pemicu.iter().filter(|k| has(k)).chain(awal.iter().filter(|k| l.starts_with(k.as_str()))).chain(frasa.iter().filter(|k| l.contains(k.as_str()))) {
            if utama || merek(k, &h) { hit_merek = true } else { hit_umum = true }
        }
        let hit = if fase == Fase::Tinggi { hit_merek } else { hit_umum && !hit_merek };
        let (butuh_aksi, butuh_kata, tolak, buang) = (teks(&e["butuh_aksi"]), teks(&e["butuh_kata"]), teks(&e["tolak"]), teks(&e["buang"]));
        if !hit || (!butuh_aksi.is_empty() && !butuh_aksi.iter().any(|a| aksi.contains(a))) || (!butuh_kata.is_empty() && !butuh_kata.iter().any(|k| has(k))) || tolak.iter().any(|k| has(k)) {
            continue;
        }
        let q = w.iter().filter(|x| !abaikan.iter().any(|a| a == *x) && !buang.iter().any(|b| b == *x)).cloned().collect::<Vec<_>>().join(" ");
        if q.is_empty() {
            continue;
        }
        let url = e["url"].as_str().filter(|u| u.starts_with("https://") || u.starts_with("http://"))?;
        let q = match e["awalan"].as_str() {
            Some(a) if !a.is_empty() => format!("{a} {q}"),
            _ => q,
        };
        return Some(url.replace("{q}", &urlencoding::encode(&q)));
    }
    None
}

/// Cadangan: layanan dengan pemicu umum (fase Rendah). Dipanggil perencana setelah aturan lokal dan pemahaman makna tidak cocok.
pub fn layanan_cadangan(s: &str) -> Option<Call> {
    let l = s.to_lowercase();
    layanan(&l, Fase::Rendah).map(|u| Call { tool: "open".into(), arg: u })
}

pub const ATURAN_BAWAAN: &str = include_str!("../data/aturan.json");

/// (aturan, awal?) — awal: tambahan Bos dan bawaan berprioritas "tinggi"; sisanya cadangan setelah aturan kode.
fn aturan_json(awal: bool) -> Vec<serde_json::Value> {
    let bawaan: Vec<serde_json::Value> = serde_json::from_str(ATURAN_BAWAAN).unwrap_or_default();
    let nama = |r: &serde_json::Value| r["nama"].as_str().unwrap_or("").to_lowercase();
    let user: Vec<serde_json::Value> = crate::data::json("aturan.json", "[]").as_array().cloned().unwrap_or_default();
    let user_nama: Vec<String> = user.iter().map(nama).collect();
    let mut out: Vec<serde_json::Value> = vec![];
    if awal {
        out.extend(user);
    }
    out.extend(bawaan.into_iter().filter(|r| !user_nama.contains(&nama(r)) && (r["prioritas"].as_str() == Some("tinggi")) == awal));
    out
}

/// Daftar tindakan satu aturan: `tindakan` [{alat,arg}] atau `alat`/`arg` tunggal; `{sisa}` diganti sisa kalimat setelah awalan.
fn tindakan(r: &serde_json::Value, sisa: &str) -> Vec<Call> {
    let satu = |x: &serde_json::Value| Some(Call { tool: x["alat"].as_str()?.to_string(), arg: x["arg"].as_str().unwrap_or("").replace("{sisa}", &clean(sisa)) });
    match r["tindakan"].as_array() {
        Some(a) => a.iter().filter_map(satu).collect(),
        None => satu(r).into_iter().collect(),
    }
}

fn cocok_awalan(s: &str, l: &str, r: &serde_json::Value) -> Option<String> {
    let aw: Vec<String> = r["awalan"].as_array()?.iter().filter_map(|x| x.as_str().map(|s| s.to_lowercase())).collect();
    after(s, l, &aw.iter().map(String::as_str).collect::<Vec<_>>()).map(|x| x.to_string())
}

/// Aturan satu langkah dari aturan.json (awalan kalimat → alat), mis. "kirim ke email saya …".
fn json_rule(s: &str, l: &str, awal: bool) -> Option<Call> {
    aturan_json(awal).iter().find_map(|r| {
        let sisa = cocok_awalan(s, l, r)?;
        // aturan berargumen tetap hanya berlaku untuk kalimat yang (hampir) sama dengan awalannya; bila masih ada isi lain
        // ("buka youtube dan carikan video X") aturan layanan/bahasa yang lebih tahu isi kalimat yang menangani.
        if !r.to_string().contains("{sisa}") && sisa.split(|c: char| !c.is_alphanumeric()).any(|w| !w.is_empty() && !crate::lex::ada("pengisi", w)) {
            return None;
        }
        let t = tindakan(r, &sisa);
        (t.len() == 1).then(|| t.into_iter().next()).flatten()
    })
}

/// `rute_niat` dari alat.json: pola regex → alat (cadangan terakhir; arg = kalimat itu sendiri).
fn rute_regex(s: &str, l: &str) -> Option<Call> {
    let d = crate::data::lapis("alat.json", crate::registry::BAWAAN);
    d["rute_niat"].as_array()?.iter().find_map(|r| {
        let re = regex::Regex::new(&format!("(?i){}", r["pola"].as_str()?)).ok()?;
        (re.is_match(l) && crate::registry::ada(r["alat"].as_str()?)).then(|| Call { tool: r["alat"].as_str().unwrap_or("").to_string(), arg: s.to_string() })
    })
}

/// Makro dari aturan.json: satu kalimat → beberapa tindakan berurutan (kalimat harus PERSIS awalan, tanpa sisa).
pub fn makro(goal: &str) -> Option<Vec<Call>> {
    let g = goal.trim().to_lowercase();
    let g = g.trim_matches(|c: char| !c.is_alphanumeric());
    // "makro pagi" / "rutinitas malam": urutan alat dari makro_bawaan di alat.json
    if let Some(nama) = ["makro ", "rutinitas ", "rutin "].iter().find_map(|p| g.strip_prefix(p)) {
        let d = crate::data::lapis("alat.json", crate::registry::BAWAAN);
        if let Some(l) = d["makro_bawaan"].as_array().and_then(|a| a.iter().find(|m| m["nama"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(nama.trim())))) {
            let c: Vec<Call> = l["langkah"].as_array().into_iter().flatten().filter_map(|x| x.as_str()).filter(|t| crate::registry::ada(t)).map(|t| Call { tool: t.to_string(), arg: String::new() }).collect();
            if c.len() > 1 {
                return Some(c);
            }
        }
    }
    aturan_json(true).into_iter().chain(aturan_json(false)).find_map(|r| {
        let r = &r;
        let t = tindakan(r, "");
        (t.len() > 1 && r["awalan"].as_array()?.iter().filter_map(|x| x.as_str()).any(|a| a.to_lowercase() == g)).then_some(t)
    })
}

/// Aturan ajaran Bos (dasbor Belajar) didahulukan, lalu aturan bawaan.
pub fn rule(s: &str) -> Option<Call> {
    crate::ajar::cocok(s).or_else(|| rule_dasar(s))
}

/// Urutan: aturan JSON awal (tambahan Bos + bawaan berprioritas tinggi) → aturan kode → aturan JSON bawaan (cadangan).
pub fn rule_dasar(s: &str) -> Option<Call> {
    let (s0, l0) = (strip_fillers(s), strip_fillers(s).to_ascii_lowercase());
    json_rule(s0, &l0, true).or_else(|| rule_kode(s)).or_else(|| json_rule(s0, &l0, false)).or_else(|| rute_regex(s0, &l0))
}

fn rule_kode(s: &str) -> Option<Call> {
    let s = strip_fillers(s);
    let l = s.to_ascii_lowercase();
    if let Some(a) = crate::daya::dari_kalimat(&l) {
        return Some(Call { tool: "daya".into(), arg: a.into() });
    }
    if let Some(u) = layanan(&l, Fase::Tinggi) {
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
    if ["resume kita", "resume tentang aku", "rangkuman tentang aku", "ringkasan tentang aku", "kamu tahu apa tentang aku", "kamu tau apa tentang aku", "kamu menganggap aku", "kamu memandang aku", "kamu melihat aku", "kamu memikirkan aku", "kamu mikirin aku", "aku ini bagi kamu"].iter().any(|k| l.contains(k)) || l.trim() == "resume" {
        return mk("resume", String::new());
    }
    if ["ayo kerja", "mulai hari kerja", "mulai kerja lengkap"].iter().any(|k| l.trim() == *k) {
        return mk("kerja", "mulai".into());
    }
    if ["akhiri kerja", "selesai kerja", "tutup hari kerja", "akhir kerja"].iter().any(|k| l.trim() == *k) {
        return mk("kerja", "akhir".into());
    }
    if ["rekap minggu ini", "rekap mingguan", "rekap seminggu"].iter().any(|k| l.contains(k)) {
        return mk("kerja", "minggu".into());
    }
    if ["rekap hari ini", "rekap kerja", "rekap harian", "laporan kerja hari ini"].iter().any(|k| l.contains(k)) {
        return mk("kerja", "rekap".into());
    }
    if ["istirahat sebentar", "waktunya istirahat", "aku mau istirahat"].iter().any(|k| l.contains(k)) {
        return mk("kerja", "istirahat".into());
    }
    if l.trim() == "mulai fokus 25 menit" {
        return mk("kerja", "fokus".into());
    }
    if l.trim() == "tunda fokus 10 menit" {
        return mk("kerja", "tunda".into());
    }
    if ["penjaga fokus aktifkan", "aktifkan penjaga fokus", "penjaga fokus on"].iter().any(|k| l.trim() == *k) {
        return mk("kerja", "penjaga on".into());
    }
    if ["penjaga fokus matikan", "matikan penjaga fokus", "penjaga fokus off"].iter().any(|k| l.trim() == *k) {
        return mk("kerja", "penjaga off".into());
    }
    if ["perbarui sekarang", "perbarui nova", "update nova", "update sekarang"].iter().any(|k| l.trim() == *k) {
        return mk("pembaruan", "pasang".into());
    }
    if l.trim() == "tunda pembaruan" {
        return mk("pembaruan", "tunda".into());
    }
    if ["cek pembaruan", "cek update", "ada pembaruan", "ada update", "periksa pembaruan"].iter().any(|k| l.contains(k)) {
        return mk("pembaruan", "cek".into());
    }
    if ["versi nova", "nova versi berapa", "versi berapa nova", "versi aplikasi"].iter().any(|k| l.contains(k)) {
        return mk("pembaruan", "versi".into());
    }
    if ["daftar tugas terjadwal", "tugas terjadwal apa saja", "lihat tugas terjadwal"].iter().any(|k| l.contains(k)) {
        return mk("jadwalkan", "daftar".into());
    }
    if let Some(r) = after(s, &l, &["hapus tugas terjadwal"]) {
        return mk("jadwalkan", format!("hapus {r}"));
    }
    if let Some(r) = after(s, &l, &["jalankan tugas terjadwal"]) {
        return mk("jadwalkan", format!("jalankan {r}"));
    }
    if ["nyalakan nova otomatis", "nova otomatis saat login", "aktifkan nyala otomatis"].iter().any(|k| l.trim() == *k) {
        return mk("jadwalkan", "latar on".into());
    }
    if ["matikan nova otomatis", "matikan nyala otomatis"].iter().any(|k| l.trim() == *k) {
        return mk("jadwalkan", "latar off".into());
    }
    if ["daftar skrip", "skrip apa saja", "lihat skrip"].iter().any(|k| l.contains(k)) {
        return mk("skrip", "daftar".into());
    }
    if let Some(r) = after(s, &l, &["jalankan skrip"]) {
        return mk("skrip", format!("jalankan {r}"));
    }
    if let Some(r) = after(s, &l, &["tampilkan skrip", "lihat kode skrip"]) {
        return mk("skrip", format!("tampil {r}"));
    }
    if let Some(r) = after(s, &l, &["hapus skrip"]) {
        return mk("skrip", format!("hapus {r}"));
    }
    if let Some(r) = after(s, &l, &["hitung jatuh tegangan", "jatuh tegangan"]) {
        return mk("kalk", format!("drop {r}"));
    }
    if let Some(r) = after(s, &l, &["luas penampang kabel", "luas kabel"]) {
        return mk("kalk", format!("luas {r}"));
    }
    if let Some(r) = after(s, &l, &["koreksi faktor daya", "kapasitor koreksi"]) {
        return mk("kalk", format!("pf {r}"));
    }
    if let Some(r) = after(s, &l, &["resistor paralel", "hambatan paralel"]) {
        return mk("kalk", format!("paralel {r}"));
    }
    if let Some(r) = after(s, &l, &["resistor seri", "hambatan seri"]) {
        return mk("kalk", format!("seri {r}"));
    }
    if let Some(r) = after(s, &l, &["kalkulator teknik", "kalk"]) {
        return mk("kalk", r.to_string());
    }
    if ["saran proyek", "saran untuk proyek", "apa yang perlu dikerjakan di proyek"].iter().any(|k| l.contains(k)) {
        return mk("saranproyek", String::new());
    }
    if ["waktu proyek minggu", "waktu per proyek minggu"].iter().any(|k| l.contains(k)) {
        return mk("kerja", "proyek minggu".into());
    }
    if ["waktu proyek", "waktu per proyek", "berapa lama di proyek"].iter().any(|k| l.contains(k)) {
        return mk("kerja", "proyek".into());
    }
    if ["ekspor laporan", "simpan laporan kerja", "buat laporan kerja"].iter().any(|k| l.contains(k)) {
        return mk("kerja", "ekspor".into());
    }
    if ["rekomendasi model", "model apa yang cocok", "model lokal apa"].iter().any(|k| l.contains(k)) {
        return mk("cerdas", "model".into());
    }
    if ["uji kecerdasan", "uji emas", "tes kecerdasan"].iter().any(|k| l.contains(k)) {
        return mk("cerdas", "uji".into());
    }
    if let Some(r) = after(s, &l, &["hapus kenangan"]) {
        return mk("cerdas", format!("memori hapus {r}"));
    }
    if let Some(r) = after(s, &l, &["pudarkan kenangan", "pudarkan memori"]) {
        return mk("cerdas", format!("memori pudar {r}"));
    }
    if let Some(r) = after(s, &l, &["cari kenangan", "cari di memori"]) {
        return mk("cerdas", format!("memori cari {r}"));
    }
    if ["daftar kenangan", "kelola memori", "lihat kenangan"].iter().any(|k| l.contains(k)) {
        return mk("cerdas", "memori".into());
    }
    if let Some(r) = after(s, &l, &["cari di dokumen", "dokumen apa kata", "kata dokumen"]) {
        return mk("cerdas", format!("dokumen {r}"));
    }
    if let Some(r) = after(s, &l, &["ucapkan", "bacakan"]) {
        return mk("cerdas", format!("ucap {r}"));
    }
    if ["jelaskan layar ini", "apa yang ada di layar", "jelaskan layar"].iter().any(|k| l.contains(k)) {
        return mk("cerdas", "layar".into());
    }
    if let Some(r) = after(s, &l, &["log audit", "riwayat tindakan", "apa yang sudah kamu lakukan"]) {
        return mk("privasi", format!("log {r}"));
    }
    if ["log audit", "riwayat tindakan", "apa yang sudah kamu lakukan"].iter().any(|k| l.trim() == *k) {
        return mk("privasi", "log".into());
    }
    if ["ekspor semua data", "ekspor data saya", "unduh data saya"].iter().any(|k| l.contains(k)) {
        return mk("privasi", "ekspor".into());
    }
    if ["hapus semua data", "hapus data saya", "hapus data pribadi"].iter().any(|k| l.contains(k)) {
        return mk("privasi", "hapus".into());
    }
    if ["terapkan aturan berkas", "jalankan aturan berkas", "pakai aturan berkas"].iter().any(|k| l.contains(k)) {
        return mk("aturanberkas", "terapkan".into());
    }
    if ["daftar aturan berkas", "lihat aturan berkas", "aturan berkas apa saja"].iter().any(|k| l.contains(k)) {
        return mk("aturanberkas", "daftar".into());
    }
    if let Some(r) = after(s, &l, &["tambah aturan berkas"]) {
        return mk("aturanberkas", format!("tambah {r}"));
    }
    if let Some(r) = after(s, &l, &["hapus aturan berkas"]) {
        return mk("aturanberkas", format!("hapus {r}"));
    }
    if l.trim() == "buat pemicu ini" {
        return mk("pemicu", "simpan".into());
    }
    if l.trim() == "batal pemicu ini" {
        return mk("pemicu", "batal".into());
    }
    if ["daftar pemicu", "pemicu apa saja", "lihat pemicu"].iter().any(|k| l.contains(k)) {
        return mk("pemicu", "daftar".into());
    }
    if ["templat pemicu", "pemicu templat", "contoh pemicu"].iter().any(|k| l.contains(k)) {
        return mk("pemicu", "templat".into());
    }
    if let Some(r) = after(s, &l, &["hapus pemicu"]) {
        return mk("pemicu", format!("hapus {r}"));
    }
    if let Some(r) = after(s, &l, &["aktifkan pemicu"]) {
        return mk("pemicu", format!("aktif {r}"));
    }
    if let Some(r) = after(s, &l, &["matikan pemicu", "nonaktifkan pemicu"]) {
        return mk("pemicu", format!("nonaktif {r}"));
    }
    if let Some(r) = after(s, &l, &["izinkan tugas"]) {
        return mk("jadwalkan", format!("izinkan {r}"));
    }
    if let Some(r) = after(s, &l, &["tolak tugas"]) {
        return mk("jadwalkan", format!("tolak {r}"));
    }
    if ["riwayat tugas", "riwayat otomasi", "riwayat jadwal"].iter().any(|k| l.contains(k)) {
        return mk("jadwalkan", "riwayat".into());
    }
    if ["templat jadwal", "jadwalkan templat", "contoh jadwal"].iter().any(|k| l.trim() == *k) {
        return mk("jadwalkan", "templat".into());
    }
    if let Some(r) = after(s, &l, &["versi keahlian"]) {
        return mk("keahlian", format!("versi {r}"));
    }
    if let Some(r) = after(s, &l, &["kembalikan keahlian"]) {
        return mk("keahlian", format!("kembali {r}"));
    }
    if let Some(r) = after(s, &l, &["uji keahlian"]) {
        return mk("keahlian", format!("uji {r}"));
    }
    if l.trim() == "ekspor keahlian" {
        return mk("keahlian", "ekspor".into());
    }
    if l.trim() == "impor keahlian ini" {
        return mk("keahlian", "impor ya".into());
    }
    if l.trim() == "batal impor keahlian" {
        return mk("keahlian", "impor batal".into());
    }
    if let Some(r) = after(s, &l, &["impor keahlian"]) {
        return mk("keahlian", format!("impor {r}"));
    }
    if l.trim() == "buat keahlian ini" {
        return mk("keahlian", "buat_calon".into());
    }
    if l.trim() == "simpan keahlian" {
        return mk("keahlian", "simpan".into());
    }
    if l.trim() == "batal keahlian" {
        return mk("keahlian", "batal".into());
    }
    if ["daftar keahlian", "keahlian apa saja", "apa saja keahlianmu", "apa saja keahlian kamu"].iter().any(|k| l.contains(k)) {
        return mk("keahlian", "daftar".into());
    }
    if let Some(r) = after(s, &l, &["hapus keahlian"]) {
        return mk("keahlian", format!("hapus {r}"));
    }
    if let Some(r) = after(s, &l, &["buat keahlian", "buatkan keahlian", "tambah keahlian", "buat skill", "pelajari cara", "ajari dirimu", "kembangkan dirimu"]) {
        return mk("keahlian", format!("buat {r}"));
    }
    if ["selera aku", "seleraku", "kesukaanku", "kesukaan aku", "apa yang aku suka", "apa yang kusuka", "analisis tontonan", "analisa tontonan", "aku suka apa"].iter().any(|k| l.contains(k)) || l.trim() == "selera" {
        return mk("selera", String::new());
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

