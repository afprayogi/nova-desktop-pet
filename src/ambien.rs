//! Kesadaran suasana (ambient): apakah Bos sedang MENGETIK, dan apakah ada MUSIK/VIDEO yang bersuara.
//! Privasi: tidak ada isi ketikan yang dibaca. Mengetik hanya disimpulkan dari "ada input tetapi kursor tidak bergerak"
//! (GetLastInputInfo + GetCursorPos). Suara hanya diukur level puncaknya (IAudioMeterInformation, satu angka 0..1); jenis
//! musik/video ditebak dari judul jendela depan. Tidak ada audio, layar, atau tombol yang direkam.
use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, Once};
use std::time::{Duration, Instant};

static TYPING: AtomicBool = AtomicBool::new(false);
static VIDEO: AtomicBool = AtomicBool::new(false);
static LAST_SOUND_MS: AtomicU64 = AtomicU64::new(0);
static LEVEL: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Level puncak audio keluar terakhir (0..1), ~10x/detik.
pub fn level() -> f32 {
    f32::from_bits(LEVEL.load(Ordering::Relaxed))
}
static START: Mutex<Option<Instant>> = Mutex::new(None);
static ONCE: Once = Once::new();

fn now_ms() -> u64 {
    START.lock().ok().and_then(|s| s.map(|t| t.elapsed().as_millis() as u64)).unwrap_or(0)
}

pub fn typing() -> bool {
    TYPING.load(Ordering::Relaxed)
}

/// "" | "musik" | "video" — hanya bila ada suara keluar dalam 3 detik terakhir.
pub fn media() -> &'static str {
    let last = LAST_SOUND_MS.load(Ordering::Relaxed);
    if last == 0 || now_ms().saturating_sub(last) > 3000 {
        ""
    } else if VIDEO.load(Ordering::Relaxed) {
        "video"
    } else {
        "musik"
    }
}

const PEAK_PS: &str = r#"
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
[Guid("C02216F6-8C67-4B5B-9D00-D008E73E0064"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IAudioMeterInformation { int GetPeakValue(out float p); }
[Guid("D666063F-1587-4E43-81F1-B948E807363F"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IMMDevice { int Activate(ref Guid id, int ctx, IntPtr p, [MarshalAs(UnmanagedType.IUnknown)] out object o); }
[Guid("A95664D2-9614-4F35-A746-DE8DB63617E6"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IMMDeviceEnumerator { int NotImpl(); int GetDefaultAudioEndpoint(int flow, int role, out IMMDevice d); }
[ComImport, Guid("BCDE0395-E52F-467C-8E3D-C4579291692E")] class MMDeviceEnumeratorComObject { }
public class NovaPeak {
  static IAudioMeterInformation m;
  public static string Get() {
    try {
      if (m == null) {
        var e = (IMMDeviceEnumerator)(new MMDeviceEnumeratorComObject());
        IMMDevice d; e.GetDefaultAudioEndpoint(0, 1, out d);
        Guid g = typeof(IAudioMeterInformation).GUID; object o; d.Activate(ref g, 23, IntPtr.Zero, out o);
        m = (IAudioMeterInformation)o;
      }
      float p; m.GetPeakValue(out p);
      return p.ToString("0.000", System.Globalization.CultureInfo.InvariantCulture);
    } catch { m = null; return "0"; }
  }
}
'@
while ($true) { [Console]::Out.WriteLine([NovaPeak]::Get()); [Console]::Out.Flush(); Start-Sleep -Milliseconds 100 }
"#;

/// Satu proses PowerShell berumur panjang (dikompilasi sekali) yang mencetak level puncak audio tiap 0,7 detik.
fn start_meter() {
    use base64::Engine as _;
    let utf16: Vec<u8> = PEAK_PS.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    let enc = base64::engine::general_purpose::STANDARD.encode(utf16);
    let mut c = std::process::Command::new("powershell");
    c.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-EncodedCommand", &enc]).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000 | 0x0000_4000); // tanpa jendela + prioritas rendah
    }
    let Ok(mut child) = c.spawn() else { return };
    let Some(out) = child.stdout.take() else { return };
    std::thread::spawn(move || {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            let p = line.trim().parse::<f32>().unwrap_or(0.0);
            LEVEL.store(p.to_bits(), Ordering::Relaxed);
            if p > 0.015 {
                LAST_SOUND_MS.store(now_ms().max(1), Ordering::Relaxed);
            }
        }
        let _ = child.kill();
    });
}

const VIDEO_HINTS: &[&str] = &["youtube", "netflix", "vlc", "prime video", "disney", "twitch", "bilibili", "vidio", "iqiyi", "viu", "wetv", ".mp4", ".mkv", ".avi", "movies", "film", "kmplayer", "potplayer"];

#[cfg(windows)]
fn foreground_title() -> String {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW};
    unsafe {
        let h = GetForegroundWindow();
        if h.is_null() {
            return String::new();
        }
        let mut buf = [0u16; 256];
        let n = GetWindowTextW(h, buf.as_mut_ptr(), 256);
        String::from_utf16_lossy(&buf[..n.max(0) as usize]).to_lowercase()
    }
}

#[cfg(windows)]
fn sample_input() -> (u32, (i32, i32)) {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
    unsafe {
        let mut li = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
        let idle = if GetLastInputInfo(&mut li) != 0 { GetTickCount().wrapping_sub(li.dwTime) } else { u32::MAX };
        let mut p = POINT { x: 0, y: 0 };
        GetCursorPos(&mut p);
        (idle, (p.x, p.y))
    }
}

/// Mulai pemantau (sekali per proses). Murah: bangun tiap 0,5 detik.
#[cfg(windows)]
pub fn start() {
    ONCE.call_once(|| {
        if let Ok(mut s) = START.lock() {
            *s = Some(Instant::now());
        }
        start_meter();
        std::thread::spawn(|| {
            let mut hist: [bool; 6] = [false; 6];
            let mut last_pos = (0, 0);
            let mut n = 0u32;
            loop {
                std::thread::sleep(Duration::from_millis(500));
                let (idle, pos) = sample_input();
                // input baru (idle mengecil) tanpa gerak kursor => kemungkinan ketikan
                let key_like = idle < 600 && pos == last_pos;
                hist.rotate_left(1);
                hist[5] = key_like;
                TYPING.store(hist.iter().filter(|b| **b).count() >= 3 && idle < 1500, Ordering::Relaxed);
                last_pos = pos;
                n += 1;
                if n % 4 == 0 && media() != "" {
                    let t = foreground_title();
                    VIDEO.store(VIDEO_HINTS.iter().any(|k| t.contains(k)), Ordering::Relaxed);
                }
            }
        });
    });
}

#[cfg(not(windows))]
pub fn start() {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn media_is_silent_by_default() {
        assert_eq!(media(), "");
        assert!(!typing());
    }
}
