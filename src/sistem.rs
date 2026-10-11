//! Kontrol sistem Windows: kecerahan layar (WMI), volume (Windows Core Audio), jam/sapaan, dan aplikasi bawaan Windows (daftar putih).
//! "Machine learning"-nya ringan dan lokal: setiap kali Bos mengatur kecerahan/volume, nilainya dicatat bersama jam; bila pada jam
//! yang sama Bos biasanya memilih nilai lain dari kondisi sekarang, NOVA MENAWARKAN (Ok/Nanti) — tidak mengubah sendiri.
use chrono::{Datelike, Local, Timelike};
use std::process::Command;

fn ps(script: &str) -> Option<String> {
    let mut c = Command::new("powershell");
    c.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let o = c.output().ok()?;
    o.status.success().then(|| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

// ---------------- jam & sapaan ----------------

pub fn salam() -> &'static str {
    match Local::now().hour() {
        4..=10 => "pagi",
        11..=14 => "siang",
        15..=17 => "sore",
        _ => "malam",
    }
}

pub fn waktu_text() -> String {
    let n = Local::now();
    let hari = ["Senin", "Selasa", "Rabu", "Kamis", "Jumat", "Sabtu", "Minggu"][n.weekday().num_days_from_monday() as usize];
    let bulan = ["Januari", "Februari", "Maret", "April", "Mei", "Juni", "Juli", "Agustus", "September", "Oktober", "November", "Desember"][n.month0() as usize];
    format!("🕒 Sekarang {hari}, {} {bulan} {} pukul {:02}:{:02} (selamat {}).", n.day(), n.year(), n.hour(), n.minute(), salam())
}

// ---------------- kecerahan ----------------

pub fn brightness_get() -> Option<i64> {
    ps("(Get-CimInstance -Namespace root/WMI -ClassName WmiMonitorBrightness -ErrorAction Stop | Select-Object -First 1).CurrentBrightness")?.parse().ok()
}

pub fn brightness_set(p: i64) -> bool {
    let p = p.clamp(5, 100);
    ps(&format!("Invoke-CimMethod -InputObject (Get-CimInstance -Namespace root/WMI -ClassName WmiMonitorBrightnessMethods -ErrorAction Stop | Select-Object -First 1) -MethodName WmiSetBrightness -Arguments @{{Timeout=1;Brightness={p}}} | Out-Null; 'ok'"))
        .is_some()
}

// ---------------- volume ----------------

const VOL_PS: &str = r#"
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
[Guid("5CDF2C82-841E-4546-9722-0CF74078229A"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IAudioEndpointVolume {
  int RegisterControlChangeNotify(IntPtr p); int UnregisterControlChangeNotify(IntPtr p); int GetChannelCount(out uint n);
  int SetMasterVolumeLevel(float l, ref Guid c); int SetMasterVolumeLevelScalar(float l, ref Guid c);
  int GetMasterVolumeLevel(out float l); int GetMasterVolumeLevelScalar(out float l);
  int SetChannelVolumeLevel(uint ch, float l, ref Guid c); int SetChannelVolumeLevelScalar(uint ch, float l, ref Guid c);
  int GetChannelVolumeLevel(uint ch, out float l); int GetChannelVolumeLevelScalar(uint ch, out float l);
  int SetMute([MarshalAs(UnmanagedType.Bool)] bool m, ref Guid c); int GetMute([MarshalAs(UnmanagedType.Bool)] out bool m);
}
[Guid("D666063F-1587-4E43-81F1-B948E807363F"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IMMDevice { int Activate(ref Guid id, int ctx, IntPtr p, [MarshalAs(UnmanagedType.IUnknown)] out object o); }
[Guid("A95664D2-9614-4F35-A746-DE8DB63617E6"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IMMDeviceEnumerator { int NotImpl(); int GetDefaultAudioEndpoint(int flow, int role, out IMMDevice d); }
[ComImport, Guid("BCDE0395-E52F-467C-8E3D-C4579291692E")] class MMDeviceEnumeratorComObject { }
public class NovaVol {
  static IAudioEndpointVolume V() {
    var e = (IMMDeviceEnumerator)(new MMDeviceEnumeratorComObject());
    IMMDevice d; e.GetDefaultAudioEndpoint(0, 1, out d);
    Guid g = typeof(IAudioEndpointVolume).GUID; object o; d.Activate(ref g, 23, IntPtr.Zero, out o);
    return (IAudioEndpointVolume)o;
  }
  public static string Run(string cmd, float val) {
    var v = V(); Guid c = Guid.Empty;
    if (cmd == "set") v.SetMasterVolumeLevelScalar(val, ref c);
    if (cmd == "mute") v.SetMute(true, ref c);
    if (cmd == "unmute") v.SetMute(false, ref c);
    float l; bool m; v.GetMasterVolumeLevelScalar(out l); v.GetMute(out m);
    return Math.Round(l * 100) + " " + (m ? "1" : "0");
  }
}
'@
"#;

/// Jalankan perintah volume; mengembalikan (persen, bisu).
fn vol_run(cmd: &str, pct: i64) -> Option<(i64, bool)> {
    let script = format!("{VOL_PS}[NovaVol]::Run('{cmd}', {:.2})", (pct.clamp(0, 100) as f32) / 100.0);
    let out = ps(&script)?;
    let mut it = out.split_whitespace();
    Some((it.next()?.parse().ok()?, it.next()? == "1"))
}

pub fn volume_get() -> Option<(i64, bool)> {
    vol_run("get", 0)
}

// ---------------- pembelajaran preferensi ----------------

fn db() -> Option<rusqlite::Connection> {
    let c = crate::assistant::db().ok()?;
    c.execute_batch("CREATE TABLE IF NOT EXISTS setelan(ts INT, kind TEXT, val INT, hour INT);").ok()?;
    Some(c)
}

fn learn(kind: &str, val: i64) {
    if let Some(c) = db() {
        let _ = c.execute("INSERT INTO setelan VALUES(strftime('%s','now'), ?1, ?2, ?3)", rusqlite::params![kind, val, Local::now().hour()]);
        let _ = c.execute("DELETE FROM setelan WHERE ts < strftime('%s','now') - 90*86400", []);
    }
}

/// Median nilai pilihan Bos pada jam serupa (±1 jam), minimal 3 contoh.
fn habit(kind: &str) -> Option<i64> {
    let c = db()?;
    let h = Local::now().hour() as i64;
    let mut st = c.prepare("SELECT val FROM setelan WHERE kind=?1 AND (hour BETWEEN ?2 AND ?3 OR hour+24 BETWEEN ?2 AND ?3 OR hour-24 BETWEEN ?2 AND ?3) ORDER BY val").ok()?;
    let v: Vec<i64> = st.query_map(rusqlite::params![kind, h - 1, h + 1], |r| r.get(0)).ok()?.flatten().collect();
    (v.len() >= 3).then(|| v[v.len() / 2])
}

/// Dipanggil berkala: tawarkan nilai kebiasaan Bos bila kondisi sekarang berbeda jauh.
pub fn suggest(tx: &crate::engine::Tx) {
    for (kind, label) in [("kecerahan", "kecerahan layar"), ("volume", "volume")] {
        let Some(want) = habit(kind) else { continue };
        let cur = if kind == "kecerahan" { brightness_get() } else { volume_get().map(|v| v.0) };
        let Some(cur) = cur else { continue };
        if (cur - want).abs() >= 15 && crate::assistant::once_key(&format!("{}{kind}{}", Local::now().format("%Y-%m-%d"), Local::now().hour() / 3)) {
            crate::saran::offer(tx, &format!("Jam segini biasanya {label} Anda {want}% (sekarang {cur}%). Mau kusesuaikan?"), &format!("{kind} {want}"), "setelan");
            return;
        }
    }
}

// ---------------- kecerahan & suara OTOMATIS ----------------

fn auto_path() -> std::path::PathBuf {
    crate::config::data_dir().join("otomatis.json")
}

fn auto_flags() -> (bool, bool) {
    std::fs::read_to_string(auto_path()).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).map(|v| (v["kecerahan"].as_bool().unwrap_or(false), v["suara"].as_bool().unwrap_or(false))).unwrap_or((false, false))
}

/// Perintah `otomatis <kecerahan|suara> <on|off>`; tanpa argumen menampilkan status.
pub fn auto_cmd(arg: &str) -> String {
    let (mut b, mut s) = auto_flags();
    let mut it = arg.split_whitespace();
    if let (Some(kind), Some(onoff)) = (it.next(), it.next()) {
        let on = onoff == "on";
        match kind {
            "kecerahan" => b = on,
            "suara" | "volume" => s = on,
            _ => return "Pilih: otomatis kecerahan on|off atau otomatis suara on|off".into(),
        }
        let _ = std::fs::write(auto_path(), serde_json::json!({"kecerahan": b, "suara": s}).to_string());
    }
    format!(
        "⚙ Otomatis — kecerahan: {} · suara: {}.\nAturan: kebiasaan Bos pada jam serupa dipakai bila sudah ada ≥3 contoh; jika belum, kurva bawaan (kecerahan: malam 19-22 → 50%, 22-06 → 30%; suara: setelah 22:00 dibatasi 30%). Perubahan paling sering sekali per 45 menit, dan tidak menimpa pengaturan manual Bos (yang justru dipelajari). Ubah: 'otomatiskan kecerahan' / 'matikan suara otomatis'.",
        if b { "AKTIF" } else { "mati" },
        if s { "AKTIF" } else { "mati" }
    )
}

static LAST_AUTO: std::sync::Mutex<Option<(String, i64, std::time::Instant)>> = std::sync::Mutex::new(None);

fn default_target(kind: &str, cur: i64) -> Option<i64> {
    let h = Local::now().hour();
    match kind {
        "kecerahan" => match h {
            22..=23 | 0..=5 => Some(30.min(cur)),
            19..=21 => Some(50.min(cur)),
            _ => None,
        },
        _ => match h {
            22..=23 | 0..=5 if cur > 40 => Some(30),
            _ => None,
        },
    }
}

/// Dipanggil berkala: terapkan kecerahan/suara otomatis bila diaktifkan. Mengembalikan pesan hanya bila ada perubahan.
pub fn auto_tick() -> Vec<String> {
    let (ab, asu) = auto_flags();
    let mut msgs = vec![];
    for (kind, on) in [("kecerahan", ab), ("volume", asu)] {
        if !on {
            continue;
        }
        let cur = if kind == "kecerahan" { brightness_get().filter(|v| *v > 0) } else { volume_get().and_then(|(v, muted)| (!muted).then_some(v)) };
        let Some(cur) = cur else { continue };
        let mut last = LAST_AUTO.lock().unwrap_or_else(|e| e.into_inner());
        // Bos mengubah sendiri sejak perubahan otomatis terakhir? -> hormati & pelajari, jeda 60 menit.
        if let Some((k, val, at)) = last.as_ref() {
            if k == kind {
                if (cur - *val).abs() >= 5 {
                    learn(kind, cur);
                    *last = Some((kind.to_string(), cur, std::time::Instant::now() + std::time::Duration::from_secs(15 * 60)));
                    continue;
                }
                if at.elapsed().as_secs() < 45 * 60 {
                    continue;
                }
            }
        }
        let want = habit(kind).or_else(|| default_target(kind, cur));
        let Some(want) = want else { continue };
        if (cur - want).abs() < 15 {
            continue;
        }
        let ok = if kind == "kecerahan" { brightness_set(want) } else { vol_run("set", want).is_some() };
        if ok {
            *last = Some((kind.to_string(), want, std::time::Instant::now()));
            msgs.push(format!("{} {} otomatis: {cur}% → {want}% (kebiasaan/jam {}). Ubah sendiri bila kurang pas; aku akan belajar.", if kind == "kecerahan" { "🔆" } else { "🔊" }, kind, Local::now().format("%H:%M")));
        }
    }
    msgs
}

// ---------------- parsing perintah & eksekusi ----------------

fn first_number(l: &str) -> Option<i64> {
    let d: String = l.chars().skip_while(|c| !c.is_ascii_digit()).take_while(|c| c.is_ascii_digit()).collect();
    d.parse().ok()
}

/// Kalimat -> argumen terstandar untuk alat `atur`: "kecerahan 40" | "kecerahan +15" | "volume -10" | "volume mute" | "volume unmute" | "kecerahan ?".
pub fn parse(l: &str) -> Option<String> {
    let kind = if ["kecerahan", "brightness", "terang layar", "redupkan", "layar lebih terang", "layar lebih redup", "layar terlalu terang", "layar terlalu gelap"].iter().any(|k| l.contains(k)) {
        "kecerahan"
    } else if ["volume", "suara", "mute", "bisukan", "senyap", "unmute", "pelankan", "keraskan", "kerasin", "speaker"].iter().any(|k| l.contains(k)) {
        "volume"
    } else {
        return None;
    };
    // hindari bentrok dengan topik lain (mis. "suara motor", "tes listening")
    if kind == "volume" && !["volume", "mute", "bisukan", "senyap", "unmute", "pelankan", "keraskan", "kerasin"].iter().any(|k| l.contains(k)) && !(l.contains("suara") && (l.contains("atur") || l.contains("naik") || l.contains("turun") || l.contains("kecil") || l.contains("besar") || l.contains("%"))) {
        return None;
    }
    if kind == "volume" {
        if ["unmute", "suarakan", "nyalakan suara", "hidupkan suara"].iter().any(|k| l.contains(k)) {
            return Some("volume unmute".into());
        }
        if ["mute", "bisukan", "senyap", "matikan suara"].iter().any(|k| l.contains(k)) {
            return Some("volume mute".into());
        }
    }
    let up = ["naik", "tambah", "lebih terang", "terangkan", "lebih keras", "keraskan", "kerasin", "lebih besar", "besarkan"].iter().any(|k| l.contains(k));
    let down = ["turun", "kurang", "redup", "gelap", "lebih pelan", "pelankan", "lebih kecil", "kecilkan", "terlalu terang", "terlalu keras"].iter().any(|k| l.contains(k));
    let n = first_number(l);
    let abs = |v: i64| Some(format!("{kind} {}", v.clamp(0, 100)));
    if l.contains("maksimal") || l.contains("penuh") || l.contains("maksimum") {
        return abs(100);
    }
    if l.contains("minimum") || l.contains("paling redup") || l.contains("paling pelan") {
        return abs(if kind == "kecerahan" { 10 } else { 5 });
    }
    match (n, up, down) {
        (Some(v), true, false) => Some(format!("{kind} +{v}")),
        (Some(v), false, true) => Some(format!("{kind} -{v}")),
        (Some(v), _, _) => abs(v),
        (None, true, false) => Some(format!("{kind} +15")),
        (None, false, true) => Some(format!("{kind} -15")),
        _ => Some(format!("{kind} ?")),
    }
}

/// Eksekusi argumen hasil `parse`.
pub fn atur(arg: &str) -> String {
    let (kind, v) = arg.split_once(' ').unwrap_or((arg, "?"));
    match kind {
        "kecerahan" => {
            let Some(cur) = brightness_get() else {
                return "Layar ini tidak mendukung kontrol kecerahan lewat Windows (umumnya monitor eksternal); atur lewat tombol monitor.".into();
            };
            let target = match v {
                "?" => return format!("🔆 Kecerahan layar sekarang {cur}%."),
                s if s.starts_with('+') || s.starts_with('-') => cur + s.parse::<i64>().unwrap_or(0),
                s => s.parse().unwrap_or(cur),
            }
            .clamp(5, 100);
            if brightness_set(target) {
                learn("kecerahan", target);
                format!("🔆 Kecerahan layar: {cur}% → {target}%.")
            } else {
                "Gagal mengatur kecerahan.".into()
            }
        }
        _ => {
            let r = match v {
                "?" => volume_get(),
                "mute" => vol_run("mute", 0),
                "unmute" => vol_run("unmute", 0),
                s => {
                    let Some((cur, _)) = volume_get() else { return "Gagal membaca volume (perangkat audio tidak ditemukan).".into() };
                    let t = if s.starts_with('+') || s.starts_with('-') { cur + s.parse::<i64>().unwrap_or(0) } else { s.parse().unwrap_or(cur) };
                    let t = t.clamp(0, 100);
                    let r = vol_run("set", t);
                    if r.is_some() {
                        learn("volume", t);
                    }
                    r
                }
            };
            match r {
                Some((p, m)) => format!("🔊 Volume {p}%{}.", if m { " (dibisukan)" } else { "" }),
                None => "Gagal mengatur volume (perangkat audio tidak ditemukan).".into(),
            }
        }
    }
}

// ---------------- jaringan ----------------

/// Wi-Fi yang sedang tersambung + jaringan di sekitar (netsh, hanya membaca).
pub fn wifi() -> String {
    let run = |args: &[&str]| -> String {
        let mut c = Command::new("netsh");
        c.args(args);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            c.creation_flags(0x0800_0000);
        }
        c.output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default()
    };
    let cur = run(&["wlan", "show", "interfaces"]);
    let pick = |t: &str, key: &str| t.lines().find(|l| l.trim_start().starts_with(key)).and_then(|l| l.split_once(':')).map(|x| x.1.trim().to_string());
    let mut out = String::from("📶 Wi-Fi\n");
    match pick(&cur, "SSID") {
        Some(s) if !s.is_empty() => out.push_str(&format!("Tersambung: {s} · sinyal {} · {} · {}\n", pick(&cur, "Signal").unwrap_or_default(), pick(&cur, "Radio type").unwrap_or_default(), pick(&cur, "Authentication").unwrap_or_default())),
        _ => out.push_str("Tidak sedang tersambung ke Wi-Fi (atau adaptor Wi-Fi mati).\n"),
    }
    let nets = run(&["wlan", "show", "networks", "mode=bssid"]);
    let (mut ssid, mut list) = (String::new(), Vec::<(String, String)>::new());
    for l in nets.lines() {
        let t = l.trim();
        if t.starts_with("SSID") && !t.starts_with("SSID ") || t.starts_with("SSID ") && t.contains(':') && !t.contains("BSSID") {
            ssid = t.split_once(':').map(|x| x.1.trim().to_string()).unwrap_or_default();
        } else if t.starts_with("Signal") && !ssid.is_empty() {
            list.push((std::mem::take(&mut ssid), t.split_once(':').map(|x| x.1.trim().to_string()).unwrap_or_default()));
        }
    }
    list.sort_by_key(|(_, s)| std::cmp::Reverse(s.trim_end_matches('%').parse::<u32>().unwrap_or(0)));
    list.dedup_by(|a, b| a.0 == b.0);
    if list.is_empty() {
        out.push_str("Tidak ada jaringan terdeteksi di sekitar.");
    } else {
        out.push_str("Di sekitar:\n");
        for (s, sig) in list.iter().take(12) {
            out.push_str(&format!("• {} — {}\n", if s.is_empty() { "(tersembunyi)" } else { s }, sig));
        }
    }
    out.trim_end().to_string()
}

// ---------------- aplikasi bawaan Windows (daftar putih) ----------------

const WINAPPS: &[(&[&str], &str, &str)] = &[
    (&["kalkulator", "calculator", "calc"], "calc", "Kalkulator"),
    (&["notepad", "catatan", "buku catatan"], "notepad", "Notepad"),
    (&["paint"], "mspaint", "Paint"),
    (&["snipping tool", "snipping", "potong layar", "screenshot"], "ms-screenclip:", "Snipping Tool"),
    (&["task manager", "pengelola tugas"], "taskmgr", "Task Manager"),
    (&["file explorer", "explorer", "penjelajah file"], "explorer", "File Explorer"),
    (&["pengaturan", "settings"], "ms-settings:", "Pengaturan"),
    (&["pengaturan wifi", "pengaturan wi-fi", "wifi", "wi-fi"], "ms-settings:network-wifi", "Pengaturan Wi-Fi"),
    (&["pengaturan bluetooth", "bluetooth"], "ms-settings:bluetooth", "Pengaturan Bluetooth"),
    (&["pengaturan suara", "pengaturan sound"], "ms-settings:sound", "Pengaturan Suara"),
    (&["pengaturan tampilan", "pengaturan layar", "display"], "ms-settings:display", "Pengaturan Tampilan"),
    (&["pengaturan baterai", "penghemat baterai"], "ms-settings:batterysaver", "Pengaturan Baterai"),
    (&["windows update", "pembaruan windows"], "ms-settings:windowsupdate", "Windows Update"),
    (&["control panel", "panel kontrol"], "control", "Control Panel"),
    (&["alarm", "jam alarm", "clock"], "ms-clock:", "Alarm & Jam"),
    (&["kalender", "calendar"], "outlookcal:", "Kalender"),
    (&["foto", "photos"], "ms-photos:", "Foto"),
    (&["perekam suara", "voice recorder"], "ms-call-recording:", "Perekam Suara"),
    (&["peta", "maps"], "bingmaps:", "Peta"),
    (&["cuaca", "weather"], "msnweather:", "Cuaca"),
    (&["wordpad"], "write", "WordPad"),
    (&["character map", "peta karakter"], "charmap", "Peta Karakter"),
    (&["device manager", "pengelola perangkat"], "devmgmt.msc", "Device Manager"),
    (&["disk management", "manajemen disk"], "diskmgmt.msc", "Manajemen Disk"),
    (&["resource monitor", "monitor sumber daya"], "resmon", "Resource Monitor"),
];

pub fn winapp_match(name: &str) -> Option<usize> {
    let n = name.trim().to_lowercase();
    WINAPPS.iter().position(|(keys, _, _)| keys.iter().any(|k| *k == n))
}

pub fn winapp(name: &str) -> String {
    let Some(i) = winapp_match(name) else { return format!("'{name}' tidak ada di daftar aplikasi bawaan yang kuizinkan.") };
    let (_, cmd, label) = WINAPPS[i];
    let mut c = Command::new("cmd");
    c.args(["/C", "start", "", cmd]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    match c.spawn() {
        Ok(_) => format!("🪟 Membuka {label}."),
        Err(e) => format!("Gagal membuka {label}: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_commands() {
        assert_eq!(parse("atur kecerahan 40").as_deref(), Some("kecerahan 40"));
        assert_eq!(parse("naikkan volume 20").as_deref(), Some("volume +20"));
        assert_eq!(parse("kecilkan volume").as_deref(), Some("volume -15"));
        assert_eq!(parse("layar terlalu terang").as_deref(), Some("kecerahan -15"));
        assert_eq!(parse("mute").as_deref(), Some("volume mute"));
        assert_eq!(parse("bisukan suara").as_deref(), Some("volume mute"));
        assert_eq!(parse("volume maksimal").as_deref(), Some("volume 100"));
        assert!(parse("jelaskan MCB").is_none());
        assert!(parse("suara motor berisik kenapa").is_none());
    }
    #[test]
    fn time_and_apps() {
        assert!(["pagi", "siang", "sore", "malam"].contains(&salam()));
        assert!(waktu_text().contains("pukul"));
        assert_eq!(winapp_match("Kalkulator"), Some(0));
        assert!(winapp_match("rm -rf").is_none());
    }
}

// ---------------- buka folder di VS Code berdasarkan nama ----------------

fn alnum(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase()
}

/// Cari folder (kedalaman ≤3) yang namanya cocok dengan `nama` di lokasi proyek umum. Mengembalikan jalur terpilih + alternatif.
pub fn cari_folder(nama: &str) -> Vec<std::path::PathBuf> {
    let q = alnum(nama);
    if q.len() < 3 {
        return vec![];
    }
    let home = dirs::home_dir().unwrap_or_default();
    let mut roots: Vec<std::path::PathBuf> = vec!["D:/repo".into(), "D:/".into(), "C:/repo".into(), home.join("source"), home.join("Documents"), home.join("Desktop"), home.join("Downloads"), home.join("repos")];
    roots.retain(|r| r.is_dir());
    let skip = ["node_modules", "target", ".git", "$RECYCLE.BIN", "System Volume Information", "Windows", "Program Files", "Program Files (x86)", "AppData"];
    let start = std::time::Instant::now();
    let mut found: Vec<(usize, std::path::PathBuf)> = vec![];
    for root in roots {
        let mut stack = vec![(root, 0usize)];
        while let Some((d, depth)) = stack.pop() {
            if start.elapsed() > std::time::Duration::from_secs(3) {
                break;
            }
            let Ok(rd) = std::fs::read_dir(&d) else { continue };
            for e in rd.flatten() {
                let Ok(ft) = e.file_type() else { continue };
                if !ft.is_dir() {
                    continue;
                }
                let name = e.file_name().to_string_lossy().to_string();
                if skip.contains(&name.as_str()) || name.starts_with('.') {
                    continue;
                }
                let n = alnum(&name);
                if n.contains(&q) {
                    found.push((n.len() - q.len(), e.path()));
                }
                if depth < 2 {
                    stack.push((e.path(), depth + 1));
                }
            }
        }
    }
    found.sort_by_key(|f| (f.0, f.1.components().count()));
    found.dedup_by(|a, b| a.1 == b.1);
    found.into_iter().map(|f| f.1).take(4).collect()
}

/// `kode <nama folder>`: buka folder proyek di VS Code.
pub fn kode(nama: &str) -> String {
    let hits = cari_folder(nama);
    let Some(p) = hits.first() else { return format!("Folder '{nama}' tidak ditemukan di D:/repo, D:/, Documents, Desktop, Downloads. Beri tahu lokasinya, mis. 'buka vs code D:/repo/nama'.") };
    let mut c = std::process::Command::new("cmd");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.raw_arg(format!("/C code \"{}\"", p.display())).creation_flags(0x0800_0000);
    }
    #[cfg(not(windows))]
    c.arg("-c").arg(format!("code '{}'", p.display()));
    let _ = c.spawn();
    let alt: Vec<String> = hits.iter().skip(1).map(|x| x.display().to_string()).collect();
    format!("💻 Membuka di VS Code: {}{}", p.display(), if alt.is_empty() { String::new() } else { format!("\nKalau bukan ini: {}", alt.join(" · ")) })
}

#[cfg(test)]
mod kode_tests {
    use super::*;
    #[test]
    fn folder_names_match_loosely() {
        assert_eq!(alnum("Digital-Nameplate_v2"), "digitalnameplatev2");
        assert!(alnum("digital nameplate").len() == 16);
    }
}

// ---------------- pembersih cache aman ----------------

fn dir_size(p: &std::path::Path) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![p.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            match e.metadata() {
                Ok(m) if m.is_dir() => stack.push(e.path()),
                Ok(m) => total += m.len(),
                Err(_) => {}
            }
        }
    }
    total
}

fn run_quiet(exe: &str, args: &[&str]) -> bool {
    let mut c = std::process::Command::new(exe);
    c.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    c.output().map(|o| o.status.success()).unwrap_or(false)
}

/// Hapus cache yang bisa diunduh ulang: Temp (berkas >1 hari), cache npm, pip, Hugging Face. Mengembalikan laporan ukuran.
pub fn bersih_aman() -> String {
    let free = || sysinfo::Disks::new_with_refreshed_list().list().iter().find(|d| d.mount_point().to_string_lossy().starts_with("C:")).map(|d| d.available_space()).unwrap_or(0);
    let before = free();
    let mut lines = vec![];
    let home = dirs::home_dir().unwrap_or_default();
    // Temp: hanya berkas lebih dari 1 hari (yang sedang dipakai dilewati)
    let tmp = std::env::temp_dir();
    let mut freed = 0u64;
    let cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(86_400);
    let mut stack = vec![tmp.clone()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let Ok(m) = e.metadata() else { continue };
            let name = e.file_name().to_string_lossy().to_lowercase();
            if m.is_dir() {
                if name != "claude" {
                    stack.push(e.path());
                }
            } else if (name.starts_with("nova_") || [".tmp", ".log", ".etl", ".dmp"].iter().any(|x| name.ends_with(x))) && m.modified().map(|t| t < cutoff).unwrap_or(false) && std::fs::remove_file(e.path()).is_ok() {
                freed += m.len();
            }
        }
    }
    lines.push(format!("Temp: {:.2} GB", freed as f64 / 1e9));
    let npm = dirs::cache_dir().map(|c| c.join("npm-cache")).unwrap_or_default();
    let n0 = dir_size(&npm);
    let ok = run_quiet(if cfg!(windows) { "npm.cmd" } else { "npm" }, &["cache", "clean", "--force"]);
    let n1 = dir_size(&npm);
    lines.push(format!("Cache npm: {:.2} GB{}", n0.saturating_sub(n1) as f64 / 1e9, if ok { "" } else { " (npm tidak tersedia)" }));
    let pip = dirs::cache_dir().map(|c| c.join("pip").join("Cache")).unwrap_or_default();
    let p0 = dir_size(&pip);
    let _ = run_quiet("python", &["-m", "pip", "cache", "purge"]);
    lines.push(format!("Cache pip: {:.2} GB", p0.saturating_sub(dir_size(&pip)) as f64 / 1e9));
    let hf = home.join(".cache").join("huggingface");
    let h0 = dir_size(&hf);
    if hf.exists() {
        let _ = std::fs::remove_dir_all(&hf);
    }
    lines.push(format!("Cache Hugging Face: {:.2} GB", h0 as f64 / 1e9));
    let after = free();
    format!("🧹 Pembersihan cache aman selesai:\n{}\nRuang C: kini bebas {:.1} GB (bertambah {:.2} GB).", lines.join("\n"), after as f64 / 1e9, after.saturating_sub(before) as f64 / 1e9)
}
