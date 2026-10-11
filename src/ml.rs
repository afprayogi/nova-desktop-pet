//! Machine learning kecil, murni Rust, belajar online, ringan untuk laptop 8 GB:
//! 1) Pengklasifikasi niat (k-NN di ruang vektor `ingat::embed`) - menerjemahkan kalimat bebas ke alat.
//!    Belajar dari setiap perintah yang berhasil dipahami aturan, jadi makin akurat dengan pemakaian.
//! 2) Rantai Markov orde-1 untuk prediksi aplikasi berikutnya dari kebiasaan Bos.
use crate::ingat::embed;
use std::collections::HashMap;
use std::sync::Mutex;

const SEED: &[(&str, &str)] = &[
    ("tanya", "apa itu pentanahan"), ("tanya", "jelaskan cara kerja motor induksi"), ("tanya", "bagaimana menghitung drop tegangan kabel"),
    ("tanya", "kenapa mcb sering trip"), ("tanya", "perbedaan mcb dan rcd"), ("tanya", "apa fungsi kapasitor bank"), ("tanya", "cara memilih ukuran kabel"),
    ("tanya", "bagaimana cara kerja plc"), ("tanya", "apa itu ladder logic"), ("tanya", "jelaskan hukum ohm"), ("tanya", "berapa arus motor 5 kw"),
    ("tanya", "apa itu borrow checker di rust"), ("tanya", "kenapa muncul error segmentation fault"), ("tanya", "bagaimana cara membuat list comprehension python"),
    ("tanya", "apa bedanya let dan const di javascript"), ("tanya", "cara membatalkan commit di git"), ("tanya", "jelaskan big o notation"),
    ("tanya", "error undefined reference saat linking"), ("tanya", "bagaimana cara debug program"), ("tanya", "apa itu pointer di c"),
    ("tanya", "cannot find module"), ("tanya", "nullpointerexception artinya apa"), ("tanya", "bagaimana cara kerja sql join"),
    ("kesehatan", "bagaimana kondisi laptop saya"), ("kesehatan", "cek cpu dan ram"), ("kesehatan", "laptop terasa lambat"), ("kesehatan", "apakah disk hampir penuh"),
    ("kesehatan", "performa komputer sekarang"), ("proses", "proses apa yang berjalan"), ("proses", "aplikasi apa yang makan memori"), ("proses", "tampilkan daftar proses"),
    ("baterai", "berapa sisa baterai"), ("baterai", "baterai tahan berapa lama lagi"), ("baterai", "kapan baterai habis"), ("baterai", "prediksi baterai laptop"),
    ("baterai", "kapan baterai penuh saat dicas"), ("agenda", "apa agenda hari ini"), ("agenda", "jadwal saya hari ini apa saja"), ("agenda", "ada acara apa nanti"),
    ("agenda", "kegiatan berikutnya apa"), ("habit", "apa kebiasaan saya"), ("habit", "aplikasi apa yang paling sering saya pakai"), ("habit", "setelah ini biasanya saya buka apa"),
    ("tidur", "berapa jam saya tidur"), ("tidur", "laporan tidur saya"), ("tidur", "apakah tidur saya cukup"), ("layar", "tolong baca isi layar saya"),
    ("layar", "apa yang ada di layar"), ("layar", "ringkas tampilan layar ini"), ("ngobrol", "halo"), ("ngobrol", "hai nova"), ("ngobrol", "selamat pagi"),
    ("ngobrol", "terima kasih banyak"), ("ngobrol", "siapa kamu"), ("ngobrol", "apa kabar"), ("ngobrol", "kamu bisa apa saja"), ("ngobrol", "bantu aku dong"),
    ("ngobrol", "makasih ya"), ("ngobrol", "oke sip"),
    ("berita", "ada berita apa hari ini"), ("berita", "kabar terbaru soal teknologi"), ("berita", "kasih aku info terbaru"), ("berita", "apa yang baru di dunia listrik"),
    ("ingatan", "apa yang kita bahas kemarin soal proyek"), ("ingatan", "kamu ingat apa tentang rencana belajar saya"), ("ingatan", "pernah kita bicarakan ini sebelumnya"),
    ("tugas", "tugas utama saya hari ini"), ("tugas", "apa tugas utamaku"), ("musik", "putarkan musik buat kerja"), ("musik", "nyalakan lagu"),
    ("email", "ada email baru"), ("email", "cek kotak masuk saya"), ("email", "apakah ada surat masuk"),
    ("apps", "matikan aplikasi yang tidak dipakai"), ("apps", "tutup program yang nganggur biar ringan"), ("apps", "bebaskan memori laptop"),
    ("rapikan", "rapikan folder download saya"), ("rapikan", "bereskan file di desktop"), ("rapikan", "susun file berdasarkan jenis"),
    ("cari", "carikan file laporan kemarin"), ("cari", "di mana file skripsi saya"), ("cari", "cari dokumen invoice pdf"),
    ("meme", "kasih aku meme"), ("meme", "hibur aku dong"), ("meme", "ada yang lucu"),
    ("kurikulum", "buatkan jadwal belajar dari buku ini"), ("kurikulum", "susun rencana belajar rust"), ("kurikulum", "aku mau belajar plc mulai besok"),
];

struct Model {
    items: Vec<(String, Vec<f32>)>,
}

static MODEL: Mutex<Option<Model>> = Mutex::new(None);

fn learned() -> Vec<(String, String)> {
    let Ok(c) = crate::teknik::open() else { return vec![] };
    c.prepare("SELECT label, text FROM intent_ex ORDER BY rowid DESC LIMIT 400")
        .and_then(|mut st| st.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).map(|r| r.flatten().collect()))
        .unwrap_or_default()
}

fn build() -> Model {
    let all = SEED.iter().map(|(l, t)| (l.to_string(), t.to_string())).chain(learned());
    Model { items: all.map(|(l, t)| (l, embed(&t))).collect() }
}

/// Niat terkuat + skor kemiripan (cosine). Metode: k-NN 1 tetangga per niat (tepat untuk data kecil).
fn norm_text(t: &str) -> String {
    t.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Tandai bahwa kalimat ini BUKAN niat `label` (Bos menekan "Salah"); juga hapus contoh belajar yang keliru.
pub fn penalize(text: &str, label: &str) {
    if let Ok(c) = crate::teknik::open() {
        let _ = c.execute_batch("CREATE TABLE IF NOT EXISTS intent_neg(text TEXT, label TEXT, PRIMARY KEY(text,label));");
        let _ = c.execute("INSERT OR IGNORE INTO intent_neg VALUES(?1,?2)", (norm_text(text), label));
        let _ = c.execute("DELETE FROM intent_ex WHERE text=?1 AND label=?2", (text.trim(), label));
        if let Ok(mut g) = MODEL.lock() {
            *g = None;
        }
    }
}

/// Semua niat yang pernah ditandai salah untuk kalimat ini.
pub fn negatives(text: &str) -> std::collections::HashSet<String> {
    let Ok(c) = crate::teknik::open() else { return Default::default() };
    let Ok(mut st) = c.prepare("SELECT label FROM intent_neg WHERE text=?1") else { return Default::default() };
    let r = st.query_map([norm_text(text)], |r| r.get::<_, String>(0)).map(|r| r.flatten().collect()).unwrap_or_default();
    r
}

fn is_negative(text: &str, label: &str) -> bool {
    crate::teknik::open()
        .ok()
        .and_then(|c| c.query_row("SELECT 1 FROM intent_neg WHERE text=?1 AND label=?2", (norm_text(text), label), |_| Ok(())).ok())
        .is_some()
}

pub fn classify(text: &str) -> Option<(String, f32)> {
    let q = embed(text);
    let mut g = MODEL.lock().ok()?;
    let m = g.get_or_insert_with(build);
    let mut best: HashMap<&str, f32> = HashMap::new();
    for (l, v) in &m.items {
        let s: f32 = v.iter().zip(&q).map(|(a, b)| a * b).sum();
        let e = best.entry(l.as_str()).or_insert(f32::MIN);
        if s > *e {
            *e = s;
        }
    }
    let mut sc: Vec<(&str, f32)> = best.into_iter().collect();
    sc.sort_by(|a, b| b.1.total_cmp(&a.1));
    let (top, second) = (sc.first()?, sc.get(1).map(|x| x.1).unwrap_or(0.0));
    let r = (top.1 >= 0.30 && top.1 - second >= 0.03).then(|| (top.0.to_string(), top.1));
    drop(g);
    r.filter(|(l, _)| !is_negative(text, l))
}

/// Belajar dari perintah yang sudah dipahami (aturan) agar kalimat serupa nanti dikenali.
pub fn learn(label: &str, text: &str) {
    let t = text.trim();
    if t.len() < 4 || t.len() > 200 {
        return;
    }
    if let Ok(c) = crate::teknik::open() {
        let dup: i64 = c.query_row("SELECT COUNT(*) FROM intent_ex WHERE text=?1", [t], |r| r.get(0)).unwrap_or(1);
        if dup == 0 {
            let _ = c.execute("INSERT INTO intent_ex VALUES(?1,?2)", (label, t));
            if let Ok(mut g) = MODEL.lock() {
                *g = None; // bangun ulang indeks contoh pada pemakaian berikutnya
            }
        }
    }
}

// ---------- Markov: aplikasi berikutnya ----------

pub fn record_transition(from: &str, to: &str) {
    if from == to || from.is_empty() {
        return;
    }
    if let Ok(c) = crate::assistant::db() {
        let _ = c.execute("INSERT INTO trans VALUES(?1,?2,1) ON CONFLICT(a,b) DO UPDATE SET n=n+1", (from, to));
    }
}

/// (aplikasi berikutnya, peluang) dari aplikasi `app`.
pub fn predict_next(app: &str) -> Option<(String, f32)> {
    let c = crate::assistant::db().ok()?;
    let rows: Vec<(String, i64)> = c
        .prepare("SELECT b, n FROM trans WHERE a=?1 ORDER BY n DESC LIMIT 5")
        .and_then(|mut st| st.query_map([app], |r| Ok((r.get(0)?, r.get(1)?))).map(|r| r.flatten().collect()))
        .ok()?;
    let total: i64 = rows.iter().map(|r| r.1).sum();
    let (b, n) = rows.first()?;
    (total >= 3).then(|| (b.clone(), *n as f32 / total as f32))
}

// ---------- Dialog state tracking (pengisian slot bertahap) ----------

static DST: Mutex<Option<(String, std::time::Instant, bool)>> = Mutex::new(None);

/// Simpan perintah yang menunggu slot (mis. jam atau path); berlaku 3 menit.
pub fn dst_set(cmd: &str) {
    if let Ok(mut d) = DST.lock() {
        *d = Some((cmd.to_string(), std::time::Instant::now(), false));
    }
}

/// Seperti dst_set, tetapi jawaban berikutnya SELALU digabung (jawaban bebas, mis. tugas utama).
pub fn dst_set_force(cmd: &str) {
    if let Ok(mut d) = DST.lock() {
        *d = Some((cmd.to_string(), std::time::Instant::now(), true));
    }
}

pub fn dst_take_full() -> Option<(String, bool)> {
    let mut d = DST.lock().ok()?;
    d.take().filter(|(_, t, _)| t.elapsed().as_secs() < 300).map(|x| (x.0, x.2))
}

// ---------- Slot filling ----------

/// Kekurangan slot untuk satu panggilan alat: pertanyaan lanjutan bila ada.
pub fn missing_slot(tool: &str, arg: &str) -> Option<&'static str> {
    let empty = arg.trim().is_empty();
    match tool {
        "remind" if !crate::assistant::has_time(arg) => Some("Jam berapa? (mis. 15:30, atau '20 menit lagi')"),
        "ringkas" | "pelajari" | "pdf" | "read" | "kurikulum" if empty => Some("File mana? Tulis path-nya (mis. C:/Users/Anda/buku.pdf)"),
        "open" if empty => Some("Mau membuka apa? (URL, file, atau nama aplikasi)"),
        _ => None,
    }
}

// ---------- Kebiasaan: jam tidak biasa ----------

/// Bila Bos aktif pada jam yang jarang ia pakai (dipelajari dari riwayat), kembalikan sapaan proaktif.
pub fn unusual_hour_checkin(c: &rusqlite::Connection) -> Option<String> {
    let now = chrono::Local::now();
    let h = chrono::Timelike::hour(&now) as i64;
    let today = now.format("%Y-%m-%d").to_string();
    if !(0..=5).contains(&h) {
        return None;
    }
    let total: i64 = c.query_row("SELECT COUNT(DISTINCT day) FROM usage WHERE day<>?1", [&today], |r| r.get(0)).unwrap_or(0);
    let at_hour: i64 = c.query_row("SELECT COUNT(DISTINCT day) FROM usage WHERE hour=?1 AND day<>?2", (h, &today), |r| r.get(0)).unwrap_or(0);
    let unusual = if total >= 4 { (at_hour as f32 / total as f32) < 0.2 } else { true };
    if !unusual || !crate::assistant::once_key(&format!("{today}malam{}", h / 3)) {
        return None;
    }
    let what = match crate::konteks::current().0 {
        crate::konteks::Mode::Coding => "debugging/coding",
        crate::konteks::Mode::Dokumen => "mengerjakan dokumen",
        crate::konteks::Mode::Teknik => "mengerjakan desain teknik",
        _ => "bekerja",
    };
    Some(format!("Sudah jam {h:02}:{:02}. Jarang sekali Bos aktif jam segini. Masih lanjut {what}? Tidur cukup bikin besok lebih tajam.", chrono::Timelike::minute(&now)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slots_and_missing() {
        assert!(missing_slot("remind", "ingatkan saya minum").is_some());
        assert!(missing_slot("remind", "ingatkan jam 15:30 minum").is_none());
        assert!(missing_slot("ringkas", "").is_some());
    }
    #[test]
    fn classifies_free_text() {
        assert_eq!(classify("kapan baterai laptop saya habis").map(|x| x.0).as_deref(), Some("baterai"));
        assert_eq!(classify("jelaskan apa itu borrow checker").map(|x| x.0).as_deref(), Some("tanya"));
        assert_eq!(classify("halo nova selamat pagi").map(|x| x.0).as_deref(), Some("ngobrol"));
        assert!(classify("xqzv wlkj").is_none());
    }
}
