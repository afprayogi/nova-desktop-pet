//! Koneksi akun dari antarmuka: status dan penerapan. Sandi/kunci dikirim langsung ke sini (bukan lewat obrolan, supaya tidak
//! tercatat di riwayat percakapan), lalu disimpan terenkripsi DPAPI. Status tidak pernah memuat sandi/kunci.
use crate::config::data_dir;
use serde_json::{json, Value};

fn brave_file() -> std::path::PathBuf {
    data_dir().join("brave.key")
}

/// Key Brave Search API: env BRAVE_API_KEY, atau yang disimpan terenkripsi lewat antarmuka.
pub fn brave_key() -> Option<String> {
    if let Ok(k) = std::env::var("BRAVE_API_KEY") {
        if !k.trim().is_empty() {
            return Some(k.trim().to_string());
        }
    }
    crate::secure::unprotect(std::fs::read_to_string(brave_file()).ok()?.trim())
}

fn smtp_info() -> Option<(String, String)> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(data_dir().join("smtp.json")).ok()?).ok()?;
    Some((v["host"].as_str()?.to_string(), v["user"].as_str()?.to_string()))
}

pub fn status() -> Value {
    let cfg = crate::config::load();
    let (bed, wake) = crate::assistant::sleep_window();
    json!({
        "gemini": {"key": crate::gemini::has_key(), "aktif": crate::gemini::ready()},
        "omniroute": {"url": cfg.base_url, "key": !cfg.api_key.is_empty()},
        "brave": {"key": brave_key().is_some()},
        "imap": crate::imap::accounts().iter().map(|a| json!({"label": a.label, "host": a.host})).collect::<Vec<_>>(),
        "smtp": smtp_info().map(|(h, u)| json!({"host": h, "user": u})),
        "google": {"siap": data_dir().join("google.json").exists(), "akun": crate::google::emails()},
        "outlook": {"siap": data_dir().join("outlook.json").exists(), "akun": crate::outlook::emails()},
        "riwayat": crate::riwayat::aktif(),
        "layar": crate::layar::enabled(),
        "cerewet": crate::adegan::level(),
        "tidur": {"jam_tidur": format!("{:02}:{:02}", bed / 60, bed % 60), "jam_bangun": format!("{:02}:{:02}", wake / 60, wake % 60)},
    })
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or("").trim()
}

fn aman(t: &str, maks: usize) -> bool {
    !t.is_empty() && t.chars().count() <= maks && !t.contains(['\n', '\r'])
}

/// (berhasil?, pesan untuk Bos). Pesan tidak pernah memuat sandi/kunci.
pub fn terapkan(v: &Value) -> (bool, String) {
    let aksi = s(v, "aksi");
    let hasil = |r: String, ok_awal: &str| (r.starts_with(ok_awal), r);
    match aksi {
        "imap_tambah" => {
            let (host, user, pass) = (if s(v, "host").is_empty() { "auto" } else { s(v, "host") }, s(v, "user"), v["pass"].as_str().unwrap_or("").trim());
            if !aman(user, 120) || !aman(host, 120) || !aman(pass, 200) {
                return (false, "Isi alamat email dan sandi aplikasi dengan benar (tanpa baris baru).".into());
            }
            hasil(crate::imap::add(host, user, pass), "Akun IMAP")
        }
        "imap_hapus" => hasil(crate::imap::remove(s(v, "label")), "Akun '"),
        "smtp" => {
            let (host, user, pass) = (if s(v, "host").is_empty() { "auto" } else { s(v, "host") }, s(v, "user"), v["pass"].as_str().unwrap_or("").trim());
            if !aman(user, 120) || !aman(host, 120) || !aman(pass, 200) {
                return (false, "Isi alamat email dan sandi aplikasi dengan benar.".into());
            }
            hasil(crate::surel::setup(host, user, pass), "SMTP disimpan")
        }
        "gemini_key" => {
            let k = s(v, "key");
            if !aman(k, 200) {
                return (false, "Key kosong atau terlalu panjang.".into());
            }
            if crate::gemini::set_key(k) { (true, "Key Gemini disimpan terenkripsi. Ketik /gemini tes di obrolan untuk mencoba.".into()) } else { (false, "Gagal menyimpan key (enkripsi Windows tidak tersedia).".into()) }
        }
        "brave_key" => {
            let k = s(v, "key");
            if !aman(k, 200) {
                return (false, "Key kosong atau terlalu panjang.".into());
            }
            match crate::secure::protect(k).map(|e| std::fs::write(brave_file(), e)) {
                Some(Ok(_)) => (true, "Key Brave Search disimpan terenkripsi. Pencarian web sekarang memakainya.".into()),
                _ => (false, "Gagal menyimpan key.".into()),
            }
        }
        "google_siapkan" => {
            let (id, secret) = (s(v, "client_id"), s(v, "client_secret"));
            if !aman(id, 200) || !aman(secret, 200) || !id.ends_with(".apps.googleusercontent.com") {
                return (false, "Client ID Google harus berakhiran .apps.googleusercontent.com. Lihat langkah penyiapan di bawah.".into());
            }
            let ok = std::fs::write(data_dir().join("google.json"), json!({"client_id": id, "client_secret": secret}).to_string()).is_ok();
            (ok, if ok { "Kredensial Google disimpan. Tekan 'Hubungkan akun Google' untuk memilih akun.".into() } else { "Gagal menyimpan.".into() })
        }
        "outlook_siapkan" => {
            let id = s(v, "client_id");
            if !aman(id, 80) {
                return (false, "Application (client) ID Microsoft kosong atau tidak valid.".into());
            }
            let ok = std::fs::write(data_dir().join("outlook.json"), json!({"client_id": id, "tenant": "common"}).to_string()).is_ok();
            (ok, if ok { "ID aplikasi Microsoft disimpan. Tekan 'Hubungkan akun Outlook' untuk memilih akun.".into() } else { "Gagal menyimpan.".into() })
        }
        "google_hapus" => (true, crate::google::disconnect(s(v, "email"))),
        "outlook_hapus" => (true, crate::outlook::disconnect(s(v, "email"))),
        "riwayat" => (true, crate::riwayat::set(v["aktif"].as_bool().unwrap_or(false))),
        "layar" => {
            crate::layar::set_enabled(v["aktif"].as_bool().unwrap_or(false));
            (true, if crate::layar::enabled() { "Kesadaran layar aktif (OCR lokal berkala, hanya kata kunci di RAM).".into() } else { "Kesadaran layar dimatikan.".into() })
        }
        "cerewet" => {
            let n = v["nilai"].as_u64().unwrap_or(1).min(2) as u8;
            crate::adegan::set_level(n);
            (true, ["Mode tenang: hanya hal penting.", "Mode normal: berguna dan jarang.", "Mode ramai: NOVA lebih sering bicara."][n as usize].into())
        }
        _ => (false, "Aksi tidak dikenal.".into()),
    }
}

/// Aksi yang butuh jaringan (uji koneksi). Pesan mengikuti pola: apa yang gagal + penyebab + langkah perbaikan.
pub async fn uji(v: &Value) -> (bool, String) {
    if s(v, "aksi") != "imap_uji" {
        return (false, "Aksi tidak dikenal.".into());
    }
    let label = s(v, "label");
    let Some(a) = crate::imap::accounts().into_iter().find(|a| a.label == label) else {
        return (false, "Akun email itu tidak ditemukan. Tambahkan lagi lewat formulir.".into());
    };
    match crate::imap::unread(&a).await {
        Ok(m) => (true, format!("Terhubung sebagai {label}; {} surat belum dibaca.", m.len())),
        Err(e) => (false, pesan_imap(&e)),
    }
}

fn pesan_imap(e: &str) -> String {
    let l = e.to_lowercase();
    if ["auth", "login", "credentials", "password", "invalid"].iter().any(|k| l.contains(k)) {
        "Tidak bisa masuk karena sandi aplikasi ditolak. Buat sandi aplikasi baru di pengaturan keamanan akun email, lalu simpan ulang di sini.".into()
    } else if ["timed out", "timeout", "dns", "resolve", "connect", "refused"].iter().any(|k| l.contains(k)) {
        "Server email tidak terjangkau. Periksa koneksi internet dan nama server IMAP, lalu coba lagi.".into()
    } else {
        format!("Tidak bisa terhubung ({}). Periksa alamat, nama server, dan sandi aplikasi.", crate::research::truncate(e, 80))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn menolak_input_berbahaya_atau_kosong() {
        assert!(!super::terapkan(&json!({"aksi": "gemini_key", "key": ""})).0);
        assert!(!super::terapkan(&json!({"aksi": "imap_tambah", "user": "a@b.com\nx", "pass": "p"})).0);
        assert!(!super::terapkan(&json!({"aksi": "google_siapkan", "client_id": "salah", "client_secret": "x"})).0);
        assert!(!super::terapkan(&json!({"aksi": "tidak-ada"})).0);
    }
}
