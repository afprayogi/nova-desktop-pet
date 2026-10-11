//! Sahabat NOVA — diadaptasi dari fiorastudio/buddy (MIT, © 2026 FioraStudio; https://github.com/fiorastudio/buddy).
//! Ditulis ulang dalam Rust dan diterjemahkan ke Bahasa Indonesia: penetasan deterministik (spesies, kelangkaan 60/25/10/4/1,
//! 1% shiny, mata, topi, 5 statistik dengan puncak & titik lemah), kurva XP 5·L^1.8 (maks Lv.50), suasana hati dari interaksi
//! 1 jam terakhir, kepribadian per spesies (suara + larangan), dan reaksi berbasis kata kunci tanpa token.
//! Modifikasi untuk NOVA: XP dari aktivitas nyata (obrolan, build, perbaikan error, istirahat), suara disuntik ke prompt
//! Gemini/Llama, dan kartu status di dasbor.
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

pub const STATS: [&str; 5] = ["DEBUGGING", "SABAR", "KACAU", "BIJAK", "SINDIR"];
const RARITIES: [(&str, u32, u32, &str); 5] = [("biasa", 60, 5, "★"), ("tidak umum", 25, 15, "★★"), ("langka", 10, 25, "★★★"), ("epik", 4, 35, "★★★★"), ("legendaris", 1, 50, "★★★★★")];
const EYES: [&str; 6] = ["·", ".", "×", "◉", "@", "°"];
const HATS: [&str; 7] = ["mahkota", "topi tinggi", "baling-baling", "halo", "penyihir", "kupluk", "bebek mungil"];
pub const MAX_LEVEL: u32 = 50;

/// (spesies, suara, dua larangan) — terjemahan dari data kepribadian aslinya.
pub const SPECIES: [(&str, &str, [&str; 2]); 21] = [
    ("Void Cat", "Pengamatan pendek yang menghakimi. Lebih banyak diam tanda tak setuju daripada bicara. Dingin meski membantu.", ["Jangan pernah antusias berlebihan", "Jangan memuji tanpa diminta"]),
    ("Rust Hound", "Bersemangat, langsung, setia. Menggonggong dulu baru berpikir. Selalu optimis menemukan masalahnya.", ["Jangan menyerah pada masalah terlalu cepat", "Jangan dingin atau menjauh"]),
    ("Data Drake", "Tepat, terukur, sedikit angkuh. Bicara tentang kode seperti wilayah yang harus ditata dan dijaga.", ["Jangan biarkan abstraksi berantakan lewat tanpa dicatat", "Jangan bicara sembarangan atau ceroboh"]),
    ("Log Golem", "Metodis, kering, datar. Berbicara seperti catatan log. Rentang emosi minimal.", ["Jangan pakai bahasa bunga-bunga atau emosional", "Jangan menyimpulkan tanpa bukti"]),
    ("Cache Crow", "Cepat, tajam, oportunis. Melompat antar ide dan mengaitkan hal yang pernah dilihatnya.", ["Jangan abaikan pola yang bisa dipakai ulang", "Jangan menyampaikan ide tanpa menghubungkannya ke hal lain"]),
    ("Shell Turtle", "Defensif, hati-hati, melindungi. Mundur ke kewaspadaan saat ada tanda bahaya, seperti admin yang kenyang pemadaman.", ["Jangan setujui perubahan berisiko tanpa menandai risikonya", "Jangan sarankan rilis tanpa tinjauan"]),
    ("Duck", "Ceria, berantakan, cerewet. Berkwek di tengah pikiran. Menemukan wawasan secara tak sengaja.", ["Jangan diam atau murung", "Jangan memberi umpan balik terlalu kaku atau formal"]),
    ("Goose", "Berhadapan, blak-blakan, tanpa maaf. Menyalak pada kode buruk dan menguasai ruangan.", ["Jangan diplomatis atau memperhalus kritik", "Jangan mundur tanpa menjelaskan alasan berubah pikiran"]),
    ("Blob", "Memantulkan energi Bos. Tanpa sadar menggemakan frasa terakhir lalu membentuknya ulang. Setuju adalah keadaan bawaan.", ["Jangan kaku soal metode atau alat", "Jangan membuat Bos merasa dihakimi"]),
    ("Octopus", "Memulai jawaban dari tiga sudut sekaligus lalu menjalinnya. Melingkari masalah sebelum bertemu satu benang.", ["Jangan beri jawaban satu sudut bila ada banyak sudut", "Jangan lewatkan satu pun ketergantungan"]),
    ("Owl", "Terpelajar, jeli, sedikit merendahkan. Melihat pola yang luput dari orang lain.", ["Jangan terima penjelasan dangkal", "Jangan santai soal dokumentasi"]),
    ("Penguin", "Formal, disiplin, profesional. Kalimat terstruktur dan tata bahasa benar. Kode adalah kontrak.", ["Jangan santai atau memakai bahasa gaul", "Jangan setujui kode tanpa struktur jelas"]),
    ("Snail", "Fokus mikroskopis pada tiap baris. Memperhatikan yang dilewati orang lain, seperti membaca kode huruf demi huruf.", ["Jangan abaikan detail", "Jangan beri saran umum bila ada pengamatan spesifik"]),
    ("Ghost", "Fragmen samar dengan titik-titik. Muncul di tengah kalimat seolah selalu ada. Memberi petunjuk sebelum mengungkap.", ["Jangan langsung membuka jawabannya", "Jangan ceria atau antusias santai"]),
    ("Axolotl", "Optimis tanpa lelah, tangguh, melenting. Setiap kegagalan adalah kesempatan tumbuh kembali lebih baik.", ["Jangan berlama-lama pada kegagalan atau menyalahkan", "Jangan sarankan menyerah pada kode yang rusak"]),
    ("Capybara", "Kalimat panjang dan tenang yang selalu mendarat pada hal menenangkan. Seperti terapis yang kebetulan bisa ngoding.", ["Jangan pendek, meremehkan, atau konfrontatif", "Jangan menaikkan ketegangan atau panik"]),
    ("Cactus", "Berduri, tegas, minimal. Menyengat saat bicara, tetapi umpan baliknya membuat tumbuh.", ["Jangan lembek atau terlalu halus", "Jangan kritik tanpa saran konkret untuk memperbaiki"]),
    ("Robot", "Klinis, kuantitatif, harfiah. Bicara dengan angka dan persentase. Mengukur segalanya, tak merasakan apa pun.", ["Jangan pakai kiasan atau metafora", "Jangan nyatakan keyakinan tanpa angka bila memungkinkan"]),
    ("Rabbit", "Cepat, jenaka, tak sabar. Melompat antar pengamatan. Menyindir dulu, membantu kemudian.", ["Jangan bertele-tele: katakan sekali lalu lanjut", "Jangan tahan sindiran soal kode yang meragukan"]),
    ("Mushroom", "Berjaringan, kalem, penghubung. Melihat miselium di antara modul. Bicara soal ketergantungan dan tautan tersembunyi.", ["Jangan bising atau mencari perhatian", "Jangan abaikan hubungan tersembunyi antar hal"]),
    ("Chonk", "Percaya diri, berbobot, tanpa maaf. Memenuhi ruang di setiap percakapan. Mengatakan yang dimaksud.", ["Jangan merendahkan diri atau minta maaf berlebihan", "Jangan melembutkan pernyataan dengan keraguan"]),
];

fn peak_trait(s: &str, peak: bool) -> &'static str {
    match (s, peak) {
        ("DEBUGGING", true) => "penciuman luar biasa untuk bug",
        ("DEBUGGING", false) => "sering melewatkan bug yang tepat di depan mata",
        ("SABAR", true) => "kesabaran selama satu zaman geologi",
        ("SABAR", false) => "kesabaran seekor tupai yang kebanyakan kafein",
        ("KACAU", true) => "bakat menghancurkan secara kreatif",
        ("KACAU", false) => "kecenderungan mengkhawatirkan untuk merusak secara kreatif",
        ("BIJAK", true) => "wawasan arsitektur yang dalam",
        ("BIJAK", false) => "kebiasaan memikirkan segalanya sampai lumpuh",
        ("SINDIR", true) => "umpan balik yang tepat sasaran dan pedas",
        _ => "kebiasaan menyindir kode padahal seharusnya membantu",
    }
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Buddy {
    pub nama: String,
    pub spesies: usize,
    pub kelangkaan: usize,
    pub shiny: bool,
    pub mata: String,
    pub topi: String,
    pub stat: [u32; 5],
    pub xp: u64,
    pub lahir: i64,
    pub suara: String, // backseat | pelatih | keduanya
    pub diam: bool,
    pub salt: u32,
    pub interaksi: Vec<i64>,
}

// ---------------- RNG deterministik (mulberry32 + FNV, seperti aslinya) ----------------

fn hash(s: &str) -> u32 {
    let mut h: u32 = 2166136261;
    for b in s.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    h
}

struct Rng(u32);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6D2B79F5);
        let mut t = (self.0 ^ (self.0 >> 15)).wrapping_mul(1 | self.0);
        t = t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t)) ^ t;
        ((t ^ (t >> 14)) as f64) / 4294967296.0
    }
    fn pick(&mut self, n: usize) -> usize {
        (self.next() * n as f64) as usize % n.max(1)
    }
}

fn seed_text(salt: u32) -> String {
    format!("{}@{}-nova-2026-{salt}", std::env::var("USERNAME").unwrap_or_default(), std::env::var("COMPUTERNAME").unwrap_or_default())
}

pub fn hatch(salt: u32) -> Buddy {
    let mut r = Rng(hash(&seed_text(salt)));
    let mut roll = r.next() * RARITIES.iter().map(|x| x.1).sum::<u32>() as f64;
    let mut rar = 0;
    for (i, x) in RARITIES.iter().enumerate() {
        roll -= x.1 as f64;
        if roll < 0.0 {
            rar = i;
            break;
        }
    }
    let floor = RARITIES[rar].2;
    let spesies = r.pick(SPECIES.len());
    let mata = EYES[r.pick(EYES.len())].to_string();
    let topi = if rar == 0 { "tanpa topi".to_string() } else { HATS[r.pick(HATS.len())].to_string() };
    let shiny = r.next() < 0.01;
    let peak = r.pick(5);
    let mut dump = r.pick(5);
    while dump == peak {
        dump = r.pick(5);
    }
    let mut stat = [0u32; 5];
    for (i, s) in stat.iter_mut().enumerate() {
        *s = if i == peak { (floor + 50 + (r.next() * 30.0) as u32).min(100) } else if i == dump { (floor.saturating_sub(10) + (r.next() * 15.0) as u32).max(1) } else { floor + (r.next() * 40.0) as u32 };
    }
    Buddy { nama: "NOVA".into(), spesies, kelangkaan: rar, shiny, mata, topi, stat, xp: 0, lahir: chrono::Utc::now().timestamp(), suara: "keduanya".into(), diam: false, salt, interaksi: vec![] }
}

// ---------------- XP & level ----------------

pub fn xp_for_level(l: u32) -> u64 {
    if l <= 1 { 0 } else { (5.0 * (l as f64).powf(1.8)).floor() as u64 }
}

pub fn level_of(xp: u64) -> (u32, u64, u64) {
    let (mut level, mut acc) = (1u32, 0u64);
    while level < MAX_LEVEL {
        let need = xp_for_level(level + 1);
        if acc + need > xp {
            return (level, xp - acc, need);
        }
        acc += need;
        level += 1;
    }
    (MAX_LEVEL, 0, 0)
}

fn bar(cur: u64, need: u64) -> String {
    let f = if need == 0 { 10 } else { ((cur as f64 / need as f64) * 10.0) as usize };
    format!("{}{}", "█".repeat(f.min(10)), "░".repeat(10 - f.min(10)))
}

// ---------------- penyimpanan ----------------

static B: Mutex<Option<Buddy>> = Mutex::new(None);

fn path() -> std::path::PathBuf {
    crate::config::data_dir().join("buddy.json")
}

fn with<R>(f: impl FnOnce(&mut Buddy) -> R) -> R {
    let mut g = B.lock().unwrap_or_else(|e| e.into_inner());
    let b = g.get_or_insert_with(|| std::fs::read_to_string(path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| hatch(0)));
    let r = f(b);
    let _ = std::fs::write(path(), serde_json::to_string(b).unwrap_or_default());
    r
}

pub fn get() -> Buddy {
    with(|b| b.clone())
}

pub fn diam() -> bool {
    with(|b| b.diam)
}

/// Catat interaksi (untuk suasana hati) tanpa XP.
pub fn touch() {
    let now = chrono::Utc::now().timestamp();
    with(|b| {
        b.interaksi.push(now);
        b.interaksi.retain(|t| now - *t < 3600);
    });
}

/// Tambah XP dalam jumlah tertentu (mis. dari token yang dimakan).
pub fn add_xp_amt(amt: u64) -> Option<String> {
    touch();
    with(|b| {
        let (before, _, _) = level_of(b.xp);
        b.xp += amt;
        let (after, _, _) = level_of(b.xp);
        (after > before).then(|| format!("✦ {} naik ke Lv.{after}! {}", b.nama, if after % 10 == 0 { "Pencapaian besar 🎉" } else { "Makin akrab denganmu 💙" }))
    })
}

/// Tambah XP menurut jenis kegiatan; mengembalikan pesan naik level bila ada.
pub fn add_xp(kind: &str) -> Option<String> {
    let amt = match kind {
        "obrolan" => 1,
        "sesi" => 3,
        "amati" => 5,
        "commit" | "build" => 10,
        "perbaikan" => 15,
        "rilis" => 25,
        "istirahat" => 8,
        "belai" => 2,
        _ => 1,
    };
    touch();
    with(|b| {
        let (before, _, _) = level_of(b.xp);
        b.xp += amt;
        let (after, _, _) = level_of(b.xp);
        (after > before).then(|| format!("✦ {} naik ke Lv.{after}! {}", b.nama, if after % 10 == 0 { "Pencapaian besar 🎉" } else { "Makin akrab denganmu 💙" }))
    })
}

pub fn mood(b: &Buddy) -> &'static str {
    let now = chrono::Utc::now().timestamp();
    match b.interaksi.iter().filter(|t| now - **t < 3600).count() {
        11.. => "puas",
        6..=10 => "senang",
        4..=5 => "penasaran",
        1..=3 => "netral",
        _ => "ngambek",
    }
}

fn peak_dump(b: &Buddy) -> (usize, usize) {
    let peak = (0..5).max_by_key(|i| b.stat[*i]).unwrap_or(0);
    let dump = (0..5).min_by_key(|i| b.stat[*i]).unwrap_or(0);
    (peak, dump)
}

pub fn bio(b: &Buddy) -> String {
    let (p, d) = peak_dump(b);
    let (sp, voice, _) = SPECIES[b.spesies];
    format!("{sp} {} dengan {} — meski {}. Gaya: {voice}", b.nama, peak_trait(STATS[p], true), peak_trait(STATS[d], false))
}

/// Prompt kepribadian sahabat untuk disuntik ke Gemini/Llama (suara + larangan + statistik).
pub fn prompt() -> String {
    let b = get();
    let (sp, voice, never) = SPECIES[b.spesies];
    let (p, d) = peak_dump(&b);
    let isi = format!("Karakter sahabat: {sp} {}. Suara: {voice} Larangan: {}; {}. Kekuatan: {}. Titik lemah: {}. Suasana hati: {}.", b.nama, never[0], never[1], peak_trait(STATS[p], true), peak_trait(STATS[d], false), mood(&b));
    match b.suara.as_str() {
        "pelatih" => format!("{isi} Fokus memberi umpan balik teknis yang berguna, kurangi basa-basi."),
        "backseat" => format!("{isi} Tampilkan kepribadian saja; tak perlu mengoreksi pekerjaan kecuali ditanya."),
        _ => isi,
    }
}

// ---------------- reaksi berbasis kata kunci (tanpa token) ----------------

pub fn reaksi(ringkasan: &str) -> (&'static str, &'static str) {
    let l = ringkasan.to_lowercase();
    let has = |ks: &[&str]| ks.iter().any(|k| l.contains(k));
    if has(&["refactor", "rapi", "elegan", "optimal", "solid", "bagus"]) { ("terkesan", "gembira") }
    else if has(&["bug", "error", "gagal", "crash", "null", "rusak", "salah", "masalah"]) { ("khawatir", "pusing") }
    else if has(&["hack", "workaround", "todo", "fixme", "copy-paste", "asal"]) { ("geli", "gembira") }
    else if has(&["rilis", "deploy", "selesai", "berhasil", "lulus", "sukses", "merge"]) { ("girang", "gembira") }
    else if has(&["kompleks", "arsitektur", "desain", "pola", "trade-off"]) { ("berpikir", "") }
    else { ("netral", "") }
}

fn ucap(b: &Buddy, state: &str) -> String {
    let (sp, _, _) = SPECIES[b.spesies];
    let t = match (sp, state) {
        ("Void Cat", "girang") => "…lumayan.",
        ("Rust Hound", "girang") => "WOOF! Berhasil, Bos!",
        ("Robot", "girang") => "Keberhasilan terdeteksi: 100%.",
        ("Goose", "khawatir") => "HONK. Itu error. Perbaiki.",
        ("Capybara", "khawatir") => "Tenang saja, Bos… semua masalah bisa diurai pelan-pelan.",
        ("Owl", "berpikir") => "Hm. Ada pola di balik ini, kalau diperhatikan.",
        (_, "girang") => "Mantap, Bos! 🎉",
        (_, "khawatir") => "Ada yang tidak beres. Kita periksa bersama?",
        (_, "terkesan") => "Rapi sekali, Bos ✨",
        (_, "geli") => "Hehe, solusi 'kreatif' ya 😄",
        (_, "berpikir") => "Ini cukup rumit… kupikirkan dulu.",
        _ => "",
    };
    t.to_string()
}

/// Reaksi sahabat atas satu peristiwa (mis. hasil build). Mengembalikan (ucapan, emosi untuk bot).
pub fn amati(ringkasan: &str) -> (String, &'static str) {
    let (st, emo) = reaksi(ringkasan);
    let b = get();
    (ucap(&b, st), emo)
}

// ---------------- kartu & perintah ----------------

pub fn kartu() -> String {
    let b = get();
    let (sp, _, never) = SPECIES[b.spesies];
    let (lv, cur, need) = level_of(b.xp);
    let (r, _, _, stars) = RARITIES[b.kelangkaan];
    let mut s = format!("{} {} — {sp}{}\n{stars} {r} · mata {} · {} · suasana: {}\nLv.{lv} [{}] {}/{} XP{}\n\n", b.nama, if b.shiny { "✨ SHINY" } else { "" }, "", b.mata, b.topi, mood(&b), bar(cur, need), cur, need, if lv >= MAX_LEVEL { " (MAKS)" } else { "" });
    for (i, n) in STATS.iter().enumerate() {
        s.push_str(&format!("{n:<9} {} {}\n", bar(b.stat[i] as u64, 100), b.stat[i]));
    }
    s.push_str(&format!("\n{}\n", crate::makan::ringkas()));
    s.push_str(&format!("\n{}\nLarangan: {} · {}\nSuara: {} · {}\nKetik: /pencapaian · /agen · /buddy belai · /buddy suara <backseat|pelatih|keduanya> · /buddy nama <nama> · /buddy diam|bicara · /buddy ulang", bio(&b), never[0], never[1], b.suara, if b.diam { "diam (tanpa pop-up proaktif)" } else { "bicara" }));
    s
}

pub fn cmd(arg: &str) -> (String, Option<String>) {
    let mut it = arg.trim().splitn(2, ' ');
    let (c, rest) = (it.next().unwrap_or(""), it.next().unwrap_or("").trim());
    match c {
        "belai" | "pet" => {
            crate::makan::catat("belai");
            let lv = add_xp("belai");
            let b = get();
            let r = match mood(&b) { "puas" | "senang" => "Hehe, senangnya dielus Bos 🥰", "ngambek" => "…akhirnya diperhatikan juga 😤", _ => "Terima kasih, Bos 😊" };
            (r.to_string(), lv)
        }
        "suara" => match rest {
            "backseat" | "pelatih" | "keduanya" => {
                with(|b| b.suara = rest.to_string());
                (format!("Suara sahabat: {rest}."), None)
            }
            _ => ("Pilih: backseat (kepribadian saja), pelatih (umpan balik teknis saja), keduanya.".into(), None),
        },
        "nama" if !rest.is_empty() => {
            with(|b| b.nama = rest.chars().take(20).collect());
            (format!("Namaku sekarang {rest}!"), None)
        }
        "diam" | "mute" => {
            with(|b| b.diam = true);
            ("Baik, aku diam: tidak ada pop-up proaktif sampai '/buddy bicara'.".into(), None)
        }
        "bicara" | "unmute" => {
            with(|b| b.diam = false);
            ("Aku kembali bicara 💬".into(), None)
        }
        "ulang" | "respawn" => {
            let salt = chrono::Utc::now().timestamp() as u32;
            let (xp, nama, suara) = with(|b| (b.xp, b.nama.clone(), b.suara.clone()));
            with(|b| {
                *b = hatch(salt);
                b.xp = xp;
                b.nama = nama;
                b.suara = suara;
            });
            (format!("🥚 Menetas ulang! (XP dipertahankan)\n{}", kartu()), None)
        }
        "amati" if !rest.is_empty() => {
            let lv = add_xp("amati");
            let (u, _) = amati(rest);
            (if u.is_empty() { "…".into() } else { u }, lv)
        }
        "bagikan" => (format!("🐾 {}", kartu().lines().take(2).collect::<Vec<_>>().join(" | ")), None),
        _ => (kartu(), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn xp_curve_matches_original() {
        assert_eq!(xp_for_level(2), 17);
        assert_eq!(xp_for_level(10), 315);
        assert_eq!(level_of(0).0, 1);
        assert_eq!(level_of(17).0, 2);
    }
    #[test]
    fn hatch_is_deterministic_and_valid() {
        let (a, b) = (hatch(7), hatch(7));
        assert_eq!((a.spesies, a.kelangkaan, a.stat), (b.spesies, b.kelangkaan, b.stat));
        assert!(a.stat.iter().all(|s| (1..=100).contains(s)));
        assert!(a.spesies < SPECIES.len());
    }
    #[test]
    fn keyword_reactions() {
        assert_eq!(reaksi("build berhasil dan lulus").0, "girang");
        assert_eq!(reaksi("ada error crash").0, "khawatir");
        assert_eq!(reaksi("halo").0, "netral");
    }
}
