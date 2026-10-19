//! NOVA di latar (Windows): ikon system tray + mesin + API lokal + desktop pet (proses terpisah).
//! Menu: Buka obrolan / Peliharaan desktop / Kesadaran layar (OCR) / Mulai otomatis saat login / Keluar.
use crate::{agents::Animal, api, config};
use std::process::{Child, Command};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIconBuilder};

const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

pub fn autostart_enabled() -> bool {
    crate::penjadwal::autostart_aktif() || Command::new("reg").args(["query", RUN_KEY, "/v", "NOVA"]).output().map(|o| o.status.success()).unwrap_or(false)
}

/// Hanya mengubah entri HKCU\...\Run milik aplikasi ini (tanpa admin), atas permintaan pengguna.
pub fn set_autostart(on: bool) -> bool {
    // Lewat Task Scheduler (mulai ulang otomatis bila mati); entri Run lama dibersihkan agar tidak ganda.
    crate::penjadwal::autostart_set(on)
}

/// Jalankan desktop pet (proses terpisah, tanpa jendela konsol).
fn spawn_pet(port: u16) -> Vec<Child> {
    use std::os::windows::process::CommandExt;
    let Ok(exe) = std::env::current_exe() else { return vec![] };
    Command::new(&exe).arg("--pet").env("NOVA_PORT", port.to_string()).creation_flags(0x0800_0000).spawn().into_iter().collect()
}

fn kill_all(pets: &mut Vec<Child>) {
    for mut c in pets.drain(..) {
        let _ = c.kill();
    }
}

fn icon() -> Icon {
    let a = Animal::Nova;
    let main = a.rgb();
    let mut rgba = vec![0u8; 16 * 16 * 4];
    for (y, row) in a.sprite().iter().enumerate() {
        for (x, ch) in row.bytes().enumerate() {
            if let Some((r, g, b)) = crate::agents::pixel_rgb(ch, main) {
                let i = (y * 16 + x) * 4;
                rgba[i..i + 4].copy_from_slice(&[r, g, b, 255]);
            }
        }
    }
    Icon::from_rgba(rgba, 16, 16).expect("ikon")
}

/// Pintasan global Ctrl+Shift+Space membuka obrolan NOVA di mana pun Bos berada.
fn start_hotkey() {
    std::thread::spawn(|| unsafe {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, VK_SPACE};
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG, WM_HOTKEY};
        // Ctrl+Shift+Space dan Ctrl+Alt+N; bila keduanya dipakai aplikasi lain, tidak ada pintasan.
        let a = RegisterHotKey(std::ptr::null_mut(), 1, MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT, VK_SPACE as u32);
        let b = RegisterHotKey(std::ptr::null_mut(), 2, MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, 0x4E);
        if a == 0 && b == 0 {
            return;
        }
        let mut m: MSG = std::mem::zeroed();
        while GetMessageW(&mut m, std::ptr::null_mut(), 0, 0) > 0 {
            if m.message == WM_HOTKEY {
                api::OPEN_CHAT.fetch_add(1, Ordering::Relaxed);
            }
        }
    });
}

pub fn run(port: u16) -> anyhow::Result<()> {
    start_hotkey();
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build()?;
    rt.spawn(async move {
        if let Err(e) = api::serve(config::load(), port).await {
            eprintln!("API gagal: {e}");
        }
    });

    let menu = Menu::new();
    let open = MenuItem::new("Buka obrolan", true, None);
    let petm = CheckMenuItem::new("Peliharaan desktop", true, true, None);
    let senyap = CheckMenuItem::new("Mode senyap (sembunyikan peliharaan & pop-up)", true, crate::gerbang::senyap(), None);
    crate::layar::load_flag();
    let ocr = CheckMenuItem::new("Kesadaran layar (OCR berkala)", true, crate::layar::enabled(), None);
    let auto = CheckMenuItem::new("Mulai otomatis saat login", true, autostart_enabled(), None);
    let quit = MenuItem::new("Keluar", true, None);
    menu.append_items(&[&open, &petm, &senyap, &ocr, &auto, &PredefinedMenuItem::separator(), &quit])?;
    let tray = TrayIconBuilder::new().with_menu(Box::new(menu)).with_tooltip("NOVA - berjalan di latar").with_icon(icon()).build()?;
    let mut senyap_kini = crate::gerbang::senyap();
    if !senyap_kini {
        crate::notify::toast("NOVA aktif", "Aku ada di system tray dan di layar. Klik aku untuk mengobrol.");
    }
    let mut pets = if senyap_kini { vec![] } else { spawn_pet(port) };
    if senyap_kini {
        petm.set_checked(false);
    }

    let event_loop = EventLoopBuilder::new().build();
    event_loop.run(move |_e, _t, cf| {
        let _keep = (&rt, &tray);
        *cf = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(250));
        // Mode senyap bisa diubah dari obrolan ("mode senyap on/off") maupun menu: ikuti berkasnya.
        let s = crate::gerbang::senyap();
        if s != senyap_kini {
            senyap_kini = s;
            senyap.set_checked(s);
            if s {
                kill_all(&mut pets);
                petm.set_checked(false);
            } else if pets.is_empty() {
                pets = spawn_pet(port);
                petm.set_checked(true);
            }
        }
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            if ev.id == open.id() {
                api::OPEN_CHAT.fetch_add(1, Ordering::Relaxed);
                if pets.is_empty() {
                    pets = spawn_pet(port);
                    petm.set_checked(true);
                }
            } else if ev.id == auto.id() {
                let want = auto.is_checked();
                if !set_autostart(want) {
                    auto.set_checked(!want);
                }
            } else if ev.id == senyap.id() {
                crate::gerbang::set_senyap(senyap.is_checked());
            } else if ev.id == petm.id() {
                kill_all(&mut pets);
                if petm.is_checked() {
                    pets = spawn_pet(port);
                }
            } else if ev.id == ocr.id() {
                crate::layar::set_enabled(ocr.is_checked());
                crate::notify::toast("NOVA", if ocr.is_checked() { "Kesadaran layar AKTIF: OCR lokal tiap 3 menit, hanya kata kunci di RAM." } else { "Kesadaran layar dimatikan." });
            } else if ev.id == quit.id() {
                kill_all(&mut pets);
                *cf = ControlFlow::Exit;
            }
        }
    })
}
