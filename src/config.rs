use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Contoh: http://localhost:20128/v1
    pub base_url: String,
    pub api_key: String,
    /// Maks. model yang diuji per sekali scan (katalog bisa jutaan, kita uji kandidat saja).
    pub scan_limit: usize,
    pub concurrency: usize,
    /// Juri memeriksa tiap jawaban sebelum ditampilkan.
    pub judge_enabled: bool,
    /// Langkah "rencana dulu baru jawab" untuk membantu model kecil bernalar.
    pub deep_think: bool,
    /// Jalankan riset web otomatis untuk topik yang meminta sumber atau info terkini.
    pub auto_research: bool,
    /// Lampirkan gambar berlisensi yang sesuai ketika Bos meminta visual.
    pub auto_images: bool,
    /// Maks. percobaan (dengan ganti model) per giliran.
    pub max_attempts: usize,
    pub turn_delay_ms: u64,
    pub max_turns: usize,
    /// Kosong = pakai DuckDuckGo (tanpa API key). Isi URL SearXNG untuk pakai instance sendiri.
    pub searxng_url: String,
    pub timeout_s: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:20128/v1".into(),
            api_key: String::new(),
            scan_limit: 60,
            concurrency: 8,
            judge_enabled: true,
            deep_think: false,
            auto_research: true,
            auto_images: true,
            max_attempts: 3,
            turn_delay_ms: 1500,
            max_turns: 60,
            searxng_url: String::new(),
            timeout_s: 45,
        }
    }
}

pub fn data_dir() -> PathBuf {
    // Tes unit memakai folder sementara sendiri agar tidak berbagi database dengan NOVA yang sedang berjalan.
    if cfg!(test) {
        // pid bisa dipakai ulang Windows antar-proses: tambahkan waktu agar tiap proses tes mulai dari folder kosong
        static D: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
        return D
            .get_or_init(|| {
                let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
                let d = std::env::temp_dir().join(format!("nova_test_{}_{t}", std::process::id()));
                let _ = std::fs::create_dir_all(&d);
                d
            })
            .clone();
    }
    let d = std::env::var("KH_DATA_DIR")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
            let (new, old) = (base.join("nova"), base.join("kerajaan-hewan"));
            // Migrasi sekali dari nama lama agar pengaturan & memori tidak hilang.
            if !new.exists() && old.exists() {
                let _ = std::fs::rename(&old, &new);
            }
            new
        });
    let _ = std::fs::create_dir_all(&d);
    d
}

/// Folder "otak": pengetahuan, buku yang dipelajari, memori percakapan, dan data bahasa (kamus aksi, CLI, layanan, aturan ajar).
/// Urutan: variabel NOVA_OTAK, lalu isi `otak_path.txt` di folder data (diatur `/otak pindah`), lalu folder data biasa.
/// Token, kunci, dan log aktivitas tetap di folder data.
pub fn otak_dir() -> PathBuf {
    if cfg!(test) {
        return data_dir();
    }
    let d = std::env::var("NOVA_OTAK")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::fs::read_to_string(data_dir().join("otak_path.txt")).ok().map(|s| PathBuf::from(s.trim())).filter(|p| !p.as_os_str().is_empty()))
        .unwrap_or_else(data_dir);
    let _ = std::fs::create_dir_all(&d);
    d
}

/// Berkas yang termasuk otak (dipindah oleh `/otak pindah`).
pub const BERKAS_OTAK: &[&str] = &["pengetahuan.db", "memori.db", "ajar.json", "kamus_aksi.json", "cli.json", "layanan.json", "lagu.json", "otak_log.json", "katalog_alat.json", "belajar.json"];

fn path() -> PathBuf {
    data_dir().join("config.json")
}

pub fn load() -> Config {
    let mut c: Config = std::fs::read_to_string(path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    if c.api_key.is_empty() {
        if let Some(k) = std::fs::read_to_string(key_path()).ok().and_then(|e| crate::secure::unprotect(e.trim())) {
            c.api_key = k;
        }
    }
    if c.api_key.is_empty() {
        if let Ok(k) = std::env::var("OMNIROUTE_API_KEY") {
            c.api_key = k;
        }
    }
    c
}

/// Key disimpan terenkripsi DPAPI di berkas terpisah; config.json tidak lagi memuat key (jika enkripsi gagal, key tetap di JSON).
fn key_path() -> PathBuf {
    data_dir().join("omni.key")
}

pub fn save(c: &Config) {
    let mut out = c.clone();
    if !c.api_key.is_empty() {
        if let Some(enc) = crate::secure::protect(&c.api_key) {
            if std::fs::write(key_path(), enc).is_ok() {
                out.api_key.clear();
            }
        }
    }
    if let Ok(s) = serde_json::to_string_pretty(&out) {
        let _ = std::fs::write(path(), s);
    }
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn missing_automation_options_preserve_existing_behavior() {
        let cfg: Config = serde_json::from_str(r#"{"base_url":"http://localhost/v1"}"#).unwrap();
        assert!(cfg.auto_research);
        assert!(cfg.auto_images);
    }
}
