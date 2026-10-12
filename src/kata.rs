//! Bahasa Indonesia untuk pemahaman perintah: (1) kata dasar (stemmer Sastrawi, MIT) agar "memutarkan/diputar/putarkan" dikenali sebagai
//! "putar"; (2) kamus aksi: kata kerja dan sinonimnya → aksi baku; (3) kamus bahasa gaul → baku (kamus-alay dari GitHub, diunduh ke
//! folder data saat dibutuhkan, tidak disertakan di repositori).
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

fn kamus() -> &'static sastrawi::Dictionary {
    static D: OnceLock<sastrawi::Dictionary> = OnceLock::new();
    D.get_or_init(sastrawi::Dictionary::new)
}

/// Kata dasar satu kata ("memutarkan" → "putar").
pub fn dasar(w: &str) -> String {
    sastrawi::Stemmer::new(kamus()).stem_word(&w.to_lowercase()).into_owned()
}

/// Kamus aksi: aksi baku → kata dasar yang berarti sama (termasuk padanan Inggris dan gaul umum).
const AKSI: &[(&str, &[&str])] = &[
    ("putar", &["putar", "main", "setel", "stel", "nyala", "dengar", "tonton", "play", "senandung", "lantun"]),
    ("buka", &["buka", "open", "jalan", "mulai", "launch", "run", "aktif", "luncur"]),
    ("tutup", &["tutup", "close", "keluar", "henti", "stop", "akhir"]),
    ("ganti", &["ganti", "ubah", "tukar", "pindah", "switch", "alih", "rubah"]),
    ("cari", &["cari", "temu", "lacak", "search", "telusur", "kejar"]),
    ("cek", &["cek", "periksa", "lihat", "tampil", "tunjuk", "status", "tengok", "pantau", "monitor"]),
    ("matikan", &["mati", "padam", "shutdown", "off"]),
    ("baca", &["baca", "bacakan", "tampilkan", "read"]),
    ("tulis", &["tulis", "simpan", "catat", "write", "save", "rekam"]),
    ("hapus", &["hapus", "buang", "delete", "bersih", "singkir"]),
    ("kirim", &["kirim", "send", "surat", "emailkan"]),
    ("hitung", &["hitung", "kalkulasi", "calculate"]),
    ("terjemah", &["terjemah", "artikan", "translate", "alih bahasa"]),
    ("unduh", &["unduh", "download", "ambil", "tarik"]),
    ("ingat", &["ingat", "ingatkan", "remind", "alarm", "jadwal"]),
];

/// Aksi baku yang muncul dalam kalimat (urut kemunculan, tanpa duplikat).
pub fn aksi(kalimat: &str) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = vec![];
    for w in kalimat.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| w.len() > 1) {
        let d = dasar(w);
        for (baku, ragam) in AKSI {
            if (ragam.contains(&d.as_str()) || ragam.contains(&w)) && !out.contains(baku) {
                out.push(baku);
            }
        }
    }
    out
}

pub fn punya_aksi(kalimat: &str, a: &str) -> bool {
    aksi(kalimat).contains(&a)
}

// ---------- kamus gaul → baku ----------

const URL_ALAY: &str = "https://raw.githubusercontent.com/nasalsabila/kamus-alay/master/colloquial-indonesian-lexicon.csv";

fn file() -> std::path::PathBuf {
    crate::config::data_dir().join("kamus_alay.tsv")
}

static GAUL: Mutex<Option<Arc<HashMap<String, String>>>> = Mutex::new(None);

fn muat_berkas() -> Arc<HashMap<String, String>> {
    let m: HashMap<String, String> = std::fs::read_to_string(file())
        .map(|t| t.lines().filter_map(|l| l.split_once('\t')).map(|(a, b)| (a.to_string(), b.to_string())).collect())
        .unwrap_or_default();
    Arc::new(m)
}

fn gaul() -> Arc<HashMap<String, String>> {
    let mut g = GAUL.lock().unwrap();
    g.get_or_insert_with(muat_berkas).clone()
}

pub fn jumlah_gaul() -> usize {
    gaul().len()
}

/// Kata yang tidak boleh ditulis ulang: nama alat, kata aksi (dan bentukannya), agar perintah tidak rusak oleh kamus gaul.
fn terlindung(w: &str) -> bool {
    crate::registry::semua().iter().any(|s| s.nama == w) || AKSI.iter().any(|(b, r)| *b == w || r.contains(&w)) || AKSI.iter().any(|(_, r)| r.contains(&dasar(w).as_str()))
}

/// Bentuk baku dari kata gaul/singkatan (None bila tidak ada di kamus atau kata itu terlindung).
pub fn baku(w: &str) -> Option<String> {
    let w = w.to_lowercase();
    if w.len() < 2 || terlindung(&w) {
        return None;
    }
    gaul().get(&w).filter(|b| **b != w).cloned()
}

/// Unduh kamus-alay (±15 ribu pasang) bila belum ada atau `paksa`. Hanya pasangan "gaul,baku" yang disimpan.
pub async fn muat_kamus(http: &reqwest::Client, paksa: bool) -> String {
    if !paksa && file().exists() {
        return format!("Kamus gaul sudah ada ({} pasang).", jumlah_gaul());
    }
    let csv = match http.get(URL_ALAY).header("User-Agent", "nova-asisten").timeout(std::time::Duration::from_secs(30)).send().await {
        Ok(r) if r.status().is_success() => r.text().await.unwrap_or_default(),
        _ => return "Gagal mengunduh kamus gaul (periksa internet).".into(),
    };
    let mut tsv = String::new();
    let mut n = 0;
    for l in csv.lines().skip(1) {
        let mut f = l.splitn(3, ',');
        let (Some(a), Some(b)) = (f.next(), f.next()) else { continue };
        let (a, b) = (a.trim().to_lowercase(), b.trim().to_lowercase());
        if a.len() >= 2 && !b.is_empty() && a != b && !a.contains(' ') && !a.contains('"') && !b.contains('"') && b.split(' ').count() <= 2 {
            tsv.push_str(&format!("{a}\t{b}\n"));
            n += 1;
        }
    }
    if n < 1000 {
        return "Format kamus gaul tidak dikenali (data terlalu sedikit); tidak disimpan.".into();
    }
    let _ = std::fs::write(file(), tsv);
    if let Ok(mut g) = GAUL.lock() {
        *g = None;
    }
    format!("Kamus gaul dimuat: {n} pasang (sumber: github.com/nasalsabila/kamus-alay).")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stems_and_actions() {
        assert_eq!(dasar("memutarkan"), "putar");
        assert_eq!(dasar("diputar"), "putar");
        assert!(punya_aksi("tolong putarkan lagu anime", "putar"));
        assert!(punya_aksi("nyalakan musik", "putar"));
        assert!(punya_aksi("ganti musik ke lagu indonesia", "ganti"));
        assert!(punya_aksi("tengok baterai", "cek"));
        assert!(!punya_aksi("selamat pagi", "putar"));
    }
    #[test]
    fn protects_commands_from_slang() {
        assert_eq!(baku("cek"), None);
        assert_eq!(baku("baterai"), None);
    }
}
