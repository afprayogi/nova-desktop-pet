//! Daya laptop cepat: tidur, hibernasi, matikan, mulai ulang, kunci layar, dan pembatalan. Matikan/mulai ulang diberi
//! jeda 30 detik agar bisa dibatalkan (`shutdown /a`); semua yang mengakhiri sesi diminta konfirmasi oleh `alat::assess`.

pub fn jalankan(arg: &str) -> String {
    let (prog, args, pesan): (&str, &[&str], &str) = match arg.trim() {
        "kunci" => ("rundll32.exe", &["user32.dll,LockWorkStation"], "Layar dikunci."),
        "tidur" => ("rundll32.exe", &["powrprof.dll,SetSuspendState", "0,1,0"], "Laptop masuk mode tidur."),
        "hibernasi" => ("shutdown.exe", &["/h"], "Laptop hibernasi."),
        "mati" => ("shutdown.exe", &["/s", "/t", "30"], "Laptop mati dalam 30 detik. Ketik 'batalkan matikan' untuk membatalkan."),
        "ulang" => ("shutdown.exe", &["/r", "/t", "30"], "Laptop mulai ulang dalam 30 detik. Ketik 'batalkan matikan' untuk membatalkan."),
        "batal" => ("shutdown.exe", &["/a"], "Pematian/mulai ulang dibatalkan (bila sedang terjadwal)."),
        x => return format!("Perintah daya tidak dikenal: {x}"),
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        return match std::process::Command::new(prog).args(args).creation_flags(0x0800_0000).spawn() {
            Ok(_) => pesan.into(),
            Err(e) => format!("Gagal menjalankan {prog}: {e}"),
        };
    }
    #[allow(unreachable_code)]
    {
        let _ = (prog, args, pesan);
        "Hanya tersedia di Windows.".into()
    }
}

/// Kalimat → argumen daya, atau None.
pub fn dari_kalimat(l: &str) -> Option<&'static str> {
    let has = |k: &str| l.contains(k);
    let benda = has("laptop") || has("komputer") || has("pc") || has("layar") || has("windows");
    if has("batalkan matikan") || has("batal matikan") || has("batalkan shutdown") || has("batalkan restart") {
        Some("batal")
    } else if has("kunci layar") || has("kunci laptop") || l == "lock" || has("lock screen") {
        Some("kunci")
    } else if has("hibernasi") || has("hibernate") {
        Some("hibernasi")
    } else if has("tidurkan") || l == "sleep" || has("mode tidur") || (has("sleep") && benda) {
        Some("tidur")
    } else if has("shutdown") || (has("matikan") && benda && !has("aplikasi")) || has("matikan sekarang") {
        Some("mati")
    } else if has("restart") || ((has("mulai ulang") || has("nyalakan ulang")) && benda) {
        Some("ulang")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::dari_kalimat as d;
    #[test]
    fn phrases() {
        assert_eq!(d("tidurkan laptop"), Some("tidur"));
        assert_eq!(d("matikan laptop"), Some("mati"));
        assert_eq!(d("restart komputer"), Some("ulang"));
        assert_eq!(d("kunci layar"), Some("kunci"));
        assert_eq!(d("batalkan matikan"), Some("batal"));
        assert_eq!(d("matikan aplikasi yang tidak dipakai"), None);
        assert_eq!(d("tidur saya semalam bagaimana"), None);
    }
}
