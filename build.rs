//! Menanam ikon dan info versi ke nova.exe (tampil di Explorer, Pengelola Tugas, dan "Aplikasi terpasang").
fn main() {
    println!("cargo:rerun-if-changed=installer/nova.ico");
    println!("cargo:rerun-if-changed=build.rs");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("installer/nova.ico");
        res.set("ProductName", "NOVA");
        res.set("FileDescription", "NOVA - asisten desktop pet");
        res.set("CompanyName", "afprayogi");
        res.set("LegalCopyright", "MIT License");
        // Tidak fatal: bila rc.exe tidak ada, exe tetap jadi (tanpa ikon).
        if let Err(e) = res.compile() {
            println!("cargo:warning=ikon tidak dipasang: {e}");
        }
    }
}
