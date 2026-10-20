#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod adblock;
mod ambien;
mod adegan;
mod agents;
mod ajar;
mod alat;
mod alatbaru;
mod data;
mod daya;
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
mod gerbang;
mod emosi;
mod ekstensi;
mod gemini;
mod engine;
mod google;
mod graf;
mod grafik;
mod imap;
mod indeks;
mod internet;
mod ingat;
mod klip;
mod info;
mod katalog;
mod kata;
mod lex;
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
mod keahlian;
mod kerja;
mod koneksi;
mod kita;
mod notify;
mod pembaruan;
mod mcp;
mod otomasi;
mod pemicu;
mod engineer;
mod cerdas;
mod audit;
mod dokumen;
mod kondisi;
mod penjadwal;
mod skrip;
mod progres;
mod pencarian;
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
mod otak;
mod paket;
mod peramban;
mod putusan;
mod rawat;
mod registry;
mod riwayat;
mod surel;
mod bangun;
mod generator;
mod harga;
mod vektor;
mod widget;
mod wikipedia;
mod makan;
mod karakter;
mod lukis;
mod omniroute;
mod ekstra;
mod buddy;
mod lokal;
mod pdfteks;
mod penasaran;
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
mod terjemah;
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

/// Peran bawaan (tertanam) untuk sinkron pengetahuan sistem.
pub(crate) fn engine_peran_bawaan() -> &'static str {
    include_str!("../data/peran.json")
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    // Dijalankan Windows Task Scheduler: satu tugas terjadwal tanpa jendela, lalu keluar.
    if let Some(i) = args.iter().position(|a| a == "--tugas") {
        std::process::exit(penjadwal::tugas_cli(args.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(0)));
    }
    // Server MCP lewat stdio untuk asisten lain: `nova --mcp`.
    if args.iter().any(|a| a == "--mcp") {
        std::process::exit(mcp::serve());
    }
    // Rilis memakai subsistem GUI (tanpa konsol); sambungkan ke konsol pemanggil agar keluaran alat pengembang terlihat.
    #[cfg(windows)]
    if ["--versi", "--buat-kunci", "--rilis", "--autostart"].iter().any(|f| args.iter().any(|a| a == f)) {
        unsafe {
            windows_sys::Win32::System::Console::AttachConsole(windows_sys::Win32::System::Console::ATTACH_PARENT_PROCESS);
        }
    }
    // Alat pengembang & info (tidak membuka jendela): lihat docs/RILIS.md
    if args.iter().any(|a| a == "--versi") {
        println!("NOVA {}", pembaruan::VERSI);
        return Ok(());
    }
    if let Some(i) = args.iter().position(|a| a == "--buat-kunci") {
        match pembaruan::buat_kunci(std::path::Path::new(args.get(i + 1).map(|s| s.as_str()).unwrap_or("kunci-rahasia"))) {
            Ok(m) => println!("{m}"),
            Err(e) => eprintln!("gagal: {e}"),
        }
        return Ok(());
    }
    if let Some(i) = args.iter().position(|a| a == "--rilis") {
        let g = |n: usize| args.get(i + n).map(|s| s.as_str()).unwrap_or("");
        match pembaruan::rilis(std::path::Path::new(g(1)), g(2), g(3)) {
            Ok(m) => println!("{m}"),
            Err(e) => {
                eprintln!("gagal: {e}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    // Pembantu admin penangkap paket (dijalankan lewat UAC oleh paket::tangkap): `nova.exe --paket <detik> <berkas.json>`.
    if let Some(i) = args.iter().position(|a| a == "--paket") {
        paket::pembantu(args.get(i + 1).and_then(|d| d.parse().ok()).unwrap_or(15), args.get(i + 2).map(|s| s.as_str()).unwrap_or("cap.json"));
        return Ok(());
    }
    if args.iter().any(|a| a == "--pet") {
        #[cfg(windows)]
        if !single_instance(r"Local\NOVA_PET") {
            return Ok(());
        }
        if let Err(e) = pet::run(port()) {
            // Komputer tanpa OpenGL yang memadai: pakai antarmuka web lokal di browser.
            eprintln!("jendela peliharaan gagal ({e}); membuka antarmuka web");
            #[cfg(windows)]
            let _ = std::process::Command::new("cmd").args(["/C", "start", "", &format!("http://127.0.0.1:{}/ui", port())]).spawn();
        }
        return Ok(());
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
