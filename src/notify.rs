//! Pop-up notifikasi Windows di luar jendela aplikasi, lewat gerbang perhatian (`gerbang`). Platform lain: tidak melakukan apa-apa.
use crate::gerbang::{self, Putusan};

/// Semua modul memanggil ini; gerbang memutuskan tampil, ditunda (jadi ringkasan nanti), atau dibuang.
pub fn toast(title: &str, body: &str) {
    match gerbang::putuskan(title) {
        Putusan::Tampil => kirim(title, body),
        Putusan::Tunda => gerbang::simpan(title, body),
        Putusan::Diam => {}
    }
}

/// Kirim langsung tanpa gerbang (dipakai gerbang sendiri untuk ringkasan).
#[cfg(windows)]
pub fn kirim(title: &str, body: &str) {
    let (t, b) = (title.to_string(), body.to_string());
    std::thread::spawn(move || {
        let _ = notify_rust::Notification::new().appname("NOVA").summary(&t).body(&b).show();
    });
}

#[cfg(not(windows))]
pub fn kirim(_title: &str, _body: &str) {}
