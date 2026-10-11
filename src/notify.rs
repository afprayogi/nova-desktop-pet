//! Pop-up notifikasi Windows di luar jendela aplikasi. Platform lain: tidak melakukan apa-apa.

#[cfg(windows)]
pub fn toast(title: &str, body: &str) {
    let (t, b) = (title.to_string(), body.to_string());
    std::thread::spawn(move || {
        let _ = notify_rust::Notification::new().appname("NOVA").summary(&t).body(&b).show();
    });
}

#[cfg(not(windows))]
pub fn toast(_title: &str, _body: &str) {}
