//! Kirim email lewat SMTP (sandi aplikasi, DPAPI). Hanya ke alamat akun sendiri: laporan/ringkasan untuk Bos.
use crate::config::data_dir;
use lettre::{transport::smtp::authentication::Credentials, Message, SmtpTransport, Transport};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Smtp {
    host: String,
    user: String,
    pass_enc: String,
}

fn file() -> std::path::PathBuf {
    data_dir().join("smtp.json")
}

/// `/smtp <host|auto> <alamat> <sandi-aplikasi>`
pub fn setup(host: &str, user: &str, pass: &str) -> String {
    let host = match (host, user.rsplit('@').next().unwrap_or("")) {
        ("auto", "gmail.com" | "googlemail.com") => "smtp.gmail.com".to_string(),
        ("auto", "outlook.com" | "hotmail.com" | "live.com") => "smtp.office365.com".to_string(),
        ("auto", "yahoo.com") => "smtp.mail.yahoo.com".to_string(),
        ("auto", _) | ("", _) => String::new(),
        (h, _) => h.to_string(),
    };
    if host.is_empty() || !user.contains('@') || pass.is_empty() {
        return "Pakai: /smtp <host|auto> <alamat-email> <sandi-aplikasi>. Contoh: /smtp auto saya@gmail.com abcd-efgh-ijkl".into();
    }
    let Some(pass_enc) = crate::secure::protect(pass) else { return "Gagal mengenkripsi sandi (DPAPI tidak tersedia).".into() };
    let s = Smtp { host: host.clone(), user: user.to_string(), pass_enc };
    match serde_json::to_string(&s).map(|j| std::fs::write(file(), j)) {
        Ok(Ok(_)) => format!("SMTP disimpan ({host}). Sandi terenkripsi (DPAPI). Laporan hanya dikirim ke {user}."),
        _ => "Gagal menyimpan pengaturan SMTP.".into(),
    }
}

/// Arg: "judul | isi". Dikirim ke alamat akun sendiri.
pub async fn kirim(arg: &str) -> String {
    let Some(s) = std::fs::read_to_string(file()).ok().and_then(|j| serde_json::from_str::<Smtp>(&j).ok()) else {
        return "SMTP belum diatur. Pakai: /smtp auto <alamat-email> <sandi-aplikasi>".into();
    };
    let Some(pass) = crate::secure::unprotect(&s.pass_enc) else { return "Sandi SMTP tidak bisa dibuka.".into() };
    let (judul, isi) = arg.split_once('|').map(|(a, b)| (a.trim().to_string(), b.trim().to_string())).unwrap_or(("Laporan NOVA".to_string(), arg.trim().to_string()));
    tokio::task::spawn_blocking(move || -> Result<String, String> {
        let me: lettre::message::Mailbox = s.user.parse().map_err(|e| format!("alamat tidak valid: {e}"))?;
        let m = Message::builder().from(me.clone()).to(me).subject(judul).body(isi).map_err(|e| e.to_string())?;
        let t = SmtpTransport::relay(&s.host).map_err(|e| e.to_string())?.credentials(Credentials::new(s.user.clone(), pass)).build();
        t.send(&m).map_err(|e| format!("gagal kirim: {e}"))?;
        Ok(format!("Terkirim ke {}.", s.user))
    })
    .await
    .unwrap_or_else(|e| Err(e.to_string()))
    .unwrap_or_else(|e| e)
}
