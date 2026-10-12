#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod adblock;
mod ambien;
mod adegan;
mod agents;
mod alat;
mod aplikasi;
mod api;
mod bandit;
mod bec;
mod bahasa;
mod baterai;
mod berkas;
mod berita;
mod assistant;
mod cli;
mod config;
mod emosi;
mod ekstensi;
mod gemini;
mod engine;
mod google;
mod graf;
mod imap;
mod indeks;
mod ingat;
mod klip;
mod kode;
mod kurikulum;
mod konteks;
mod layar;
mod llm;
mod makro;
mod meme;
mod memory;
mod ml;
mod monitor;
mod musik;
mod notify;
mod outlook;
mod omni;
mod pet;
mod proyek;
mod research;
mod router;
mod hewan;
mod cadangan;
mod analitik;
mod aturan;
mod fakta;
mod daring;
mod gambar;
mod perkakas;
mod ilmiah;
mod niat;
mod peramban;
mod rawat;
mod surel;
mod bangun;
mod generator;
mod harga;
mod widget;
mod makan;
mod karakter;
mod lukis;
mod omniroute;
mod ekstra;
mod buddy;
mod lokal;
mod persona;
mod belajar;
mod jurnal;
mod rencana;
mod semantik;
mod sholat;
mod saran;
mod seret;
mod sistem;
mod secure;
mod skills;
mod solusi;
mod teknik;
mod tugas;
#[cfg(windows)]
mod tray;

/// Port API lokal (hanya 127.0.0.1) antara proses tray dan peliharaan.
pub fn port() -> u16 {
    std::env::var("NOVA_PORT").or_else(|_| std::env::var("PORT")).ok().and_then(|p| p.parse().ok()).unwrap_or(47831)
}

/// Satu salinan per jenis proses (pet / tray): salinan kedua langsung keluar, sehingga bot tidak pernah ganda.
#[cfg(windows)]
fn single_instance(name: &str) -> bool {
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows_sys::Win32::System::Threading::CreateMutexW;
    let w: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    unsafe {
        let h = CreateMutexW(std::ptr::null(), 1, w.as_ptr()); // handle sengaja tidak ditutup: hidup selama proses
        !h.is_null() && GetLastError() != ERROR_ALREADY_EXISTS
    }
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--pet") {
        #[cfg(windows)]
        if !single_instance(r"Local\NOVA_PET") {
            return Ok(());
        }
        return pet::run(port());
    }
    #[cfg(windows)]
    {
        if let Some(i) = args.iter().position(|a| a == "--autostart") {
            let on = args.get(i + 1).map(|s| s != "off").unwrap_or(true);
            println!("autostart {}: {}", if on { "on" } else { "off" }, if tray::set_autostart(on) { "ok" } else { "gagal" });
            return Ok(());
        }
        if !single_instance(r"Local\NOVA_TRAY") {
            return Ok(());
        }
        tray::run(port())
    }
    #[cfg(not(windows))]
    {
        println!("NOVA saat ini hanya mendukung Windows.");
        Ok(())
    }
}
