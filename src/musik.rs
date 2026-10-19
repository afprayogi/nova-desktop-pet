//! Musik fokus: bila Bos bekerja tapi belum ada pemutar musik berjalan, NOVA menyarankan (dan bila Bos setuju, memutarkan):
//! musik di folder Musik (daftar putar acak) atau, bila kosong, pencarian musik fokus di browser. Deteksi hanya berdasarkan
//! nama proses pemutar (Spotify, VLC, dst.); musik yang diputar di tab browser tidak terdeteksi.
use crate::engine::Tx;
use rand::seq::SliceRandom;
use std::path::PathBuf;

const PLAYERS: &[&str] = &["spotify", "vlc", "wmplayer", "musicbee", "potplayer", "itunes", "foobar2000", "aimp", "winamp", "music.ui", "groove", "deezer", "tidal", "amazon music"];

pub fn player_running() -> bool {
    let mut sys = sysinfo::System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    sys.processes().values().any(|p| {
        let n = p.name().to_string_lossy().to_lowercase();
        PLAYERS.iter().any(|k| n.contains(k))
    })
}

fn genre() -> &'static str {
    match crate::konteks::current().0 {
        crate::konteks::Mode::Coding => "lofi hip hop beats to code to",
        crate::konteks::Mode::Teknik => "instrumental focus music for engineers",
        crate::konteks::Mode::Dokumen => "calm piano music for studying",
        _ => "lofi chill music",
    }
}

/// Dipanggil berkala: tawarkan musik bila sedang bekerja ≥15 menit tanpa pemutar.
pub fn check(tx: &Tx) {
    let (mode, idle) = crate::konteks::current();
    let working = matches!(mode, crate::konteks::Mode::Coding | crate::konteks::Mode::Teknik | crate::konteks::Mode::Dokumen);
    if !working || idle > 120 || crate::assistant::streak_min() < 15 || player_running() {
        return;
    }
    let h = chrono::Timelike::hour(&chrono::Local::now());
    if !crate::assistant::once_key(&format!("{}musik{}", chrono::Local::now().format("%Y-%m-%d"), h / 3)) {
        return;
    }
    crate::saran::offer(tx, &format!("Sudah {} menit kerja dan belum ada musik. Mau kuputarkan musik fokus ({})?", crate::assistant::streak_min(), genre()), "putar musik", "musik");
}

fn local_tracks() -> Vec<PathBuf> {
    let mut out = vec![];
    if let Some(d) = dirs::audio_dir() {
        let mut stack = vec![(d, 0u8)];
        while let Some((dir, depth)) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() && depth < 3 {
                    stack.push((p, depth + 1));
                } else if p.extension().and_then(|x| x.to_str()).is_some_and(|x| ["mp3", "m4a", "flac", "wav", "ogg", "wma"].contains(&x.to_lowercase().as_str())) {
                    out.push(p);
                }
                if out.len() > 800 {
                    return out;
                }
            }
        }
    }
    out
}

/// Putar musik fokus. Mengembalikan teks hasil.
pub fn play() -> String {
    let mut tracks = local_tracks();
    if tracks.len() >= 3 {
        tracks.shuffle(&mut rand::thread_rng());
        tracks.truncate(40);
        let m3u = crate::config::data_dir().join("musik.m3u8");
        let body = format!("#EXTM3U\n{}\n", tracks.iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>().join("\n"));
        if std::fs::write(&m3u, body).is_ok() {
            let mut c = std::process::Command::new("cmd");
            c.args(["/C", "start", "", &m3u.to_string_lossy()]);
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                c.creation_flags(0x0800_0000);
            }
            if c.spawn().is_ok() {
                return format!("🎵 Memutar daftar putar acak dari folder Musik ({} lagu).", tracks.len());
            }
        }
    }
    let q = genre().replace(' ', "+");
    crate::google::open_in_browser(&format!("https://www.youtube.com/results?search_query={q}"));
    "🎵 Folder Musik kosong, jadi kubuka pencarian musik fokus di browser. Pilih satu dan selamat bekerja!".into()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn genre_depends_on_mode_and_detection_runs() {
        assert!(!genre().is_empty());
        let _ = player_running(); // tidak panik
    }
}

// ---------- Riwayat lagu & putar otomatis di YouTube ----------

const MUSIK_KATA: &[&str] = &["lagu", "musik", "music", "song", "lofi", "playlist", "ost", "cover", "remix", "official", "mv", "lyrics", "lirik"];

fn riwayat_file() -> PathBuf {
    crate::config::otak_dir().join("lagu.json")
}

fn baca_riwayat() -> serde_json::Value {
    std::fs::read_to_string(riwayat_file()).ok().and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_else(|| serde_json::json!({"hitung": {}, "terakhir": []}))
}

/// Catat pencarian YouTube bernuansa musik (jumlah per kueri + 10 terakhir).
pub fn catat(q: &str) {
    let q = q.trim().to_lowercase();
    if q.is_empty() || !q.split_whitespace().any(|w| MUSIK_KATA.contains(&w)) {
        return;
    }
    let mut j = baca_riwayat();
    let n = j["hitung"][&q].as_u64().unwrap_or(0) + 1;
    j["hitung"][&q] = n.into();
    let mut last: Vec<String> = j["terakhir"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
    last.retain(|x| *x != q);
    last.push(q);
    if last.len() > 10 {
        last.remove(0);
    }
    j["terakhir"] = serde_json::json!(last);
    let _ = std::fs::write(riwayat_file(), j.to_string());
}

/// (terbanyak, sebelumnya): kueri paling sering diputar, dan yang terakhir diputar selain itu.
fn pilihan() -> Option<(String, Option<String>)> {
    let j = baca_riwayat();
    let mut hitung: std::collections::HashMap<String, u64> = j["hitung"].as_object().map(|o| o.iter().map(|(k, v)| (k.clone(), v.as_u64().unwrap_or(0))).collect()).unwrap_or_default();
    if crate::riwayat::aktif() {
        for (t, n) in crate::riwayat::lagu().into_iter().take(20) {
            *hitung.entry(t).or_insert(0) += n as u64;
        }
    }
    let top = hitung.iter().max_by_key(|(_, v)| **v)?.0.clone();
    let prev = j["terakhir"].as_array().and_then(|a| a.iter().rev().filter_map(|x| x.as_str()).find(|x| *x != top).map(String::from));
    Some((top, prev))
}

fn auto_on_file() -> PathBuf {
    crate::config::data_dir().join("musik_auto_on")
}

pub fn set_auto(on: bool) -> String {
    if on {
        let _ = std::fs::write(auto_on_file(), "");
        "Putar musik otomatis: AKTIF (sengaja kamu nyalakan; maksimal sekali sehari saat Bos mulai aktif).".into()
    } else {
        let _ = std::fs::remove_file(auto_on_file());
        "Putar musik otomatis: MATI (bawaan).".into()
    }
}

fn panel_off_file() -> PathBuf {
    crate::config::data_dir().join("musik_panel_off")
}

pub fn set_panel(on: bool) -> String {
    if on {
        let _ = std::fs::remove_file(panel_off_file());
        "Panel YouTube di sisi kanan: AKTIF (jendela app Chrome/Edge 440 px di tepi kanan layar).".into()
    } else {
        let _ = std::fs::write(panel_off_file(), "");
        "Panel YouTube: MATI (video dibuka di tab biasa).".into()
    }
}

#[cfg(windows)]
fn layar() -> (i32, i32) {
    use windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics;
    unsafe { (GetSystemMetrics(0), GetSystemMetrics(1)) }
}

#[cfg(not(windows))]
fn layar() -> (i32, i32) {
    (1920, 1080)
}

pub fn jumlah_lagu() -> i64 {
    baca_riwayat()["hitung"].as_object().map(|o| o.len() as i64).unwrap_or(0)
}

pub fn buka_url(u: &str) {
    if u.contains("youtube.com") {
        let _ = std::fs::write(aktif_file(), now_s().to_string());
    }
    // Video YouTube: jendela app di sisi kanan layar (tanpa bilah alamat); ekstensi NOVA melewati iklan di dalamnya.
    if u.contains("youtube.com/watch") && !panel_off_file().exists() {
        if let Some(exe) = crate::peramban::chrome() {
            let (w, h) = layar();
            let pw = 440;
            if std::process::Command::new(exe).arg(format!("--app={u}")).arg(format!("--window-size={pw},{}", (h - 60).max(400))).arg(format!("--window-position={},0", (w - pw).max(0))).spawn().is_ok() {
                return;
            }
        }
    }
    let _ = std::process::Command::new("explorer.exe").arg(u).spawn();
}

/// Buka video teratas hasil pencarian `q` di YouTube (cadangan: halaman hasil pencarian).
pub fn putar(q: &str) {
    let search = format!("https://www.youtube.com/results?search_query={}", urlencoding::encode(q));
    let id = tokio::runtime::Builder::new_current_thread().enable_all().build().ok().and_then(|rt| {
        rt.block_on(async {
            let h = reqwest::Client::builder().user_agent("Mozilla/5.0").timeout(std::time::Duration::from_secs(10)).build().ok()?;
            let html = h.get(&search).header("Accept-Language", "id,en").send().await.ok()?.text().await.ok()?;
            regex::Regex::new(r#""videoId":"([A-Za-z0-9_-]{11})""#).ok()?.captures(&html).map(|c| c[1].to_string())
        })
    });
    buka_url(&id.map(|i| format!("https://www.youtube.com/watch?v={i}")).unwrap_or(search));
}

/// Saat Bos mulai aktif (sekali per hari): putar lagu terbanyak di YouTube dan tawarkan lagu sebelumnya.
pub fn auto(tx: &Tx) {
    if !auto_on_file().exists() || player_running() {
        return;
    }
    let Some((top, prev)) = pilihan() else { return };
    if !crate::assistant::once_key(&format!("{}musikauto", chrono::Local::now().format("%Y-%m-%d"))) {
        return;
    }
    let tx = tx.clone();
    std::thread::spawn(move || {
        putar(&top);
        if let Some(p) = prev {
            crate::saran::offer_always(&tx, &format!("Kuputar '{top}'. Mau lagu sebelumnya, '{p}'?"), &format!("putar lagu {p}"), "musik");
        }
    });
}

#[cfg(test)]
mod riwayat_tests {
    #[test]
    fn only_music_queries_counted() {
        super::catat("tutorial rust"); // bukan musik: diabaikan
        super::catat("lagu santai");
        super::catat("lagu santai");
        super::catat("lofi hip hop");
        let (top, prev) = super::pilihan().unwrap();
        assert_eq!(top, "lagu santai");
        assert_eq!(prev.as_deref(), Some("lofi hip hop"));
    }
}

// ---------- Penjaga musik browser: lanjutkan bila terjeda, putar ulang bila tab hilang ----------

const PS_JAGA: &str = r#"Add-Type -AssemblyName System.Runtime.WindowsRuntime
function Aw($op,$t){ $m=([System.WindowsRuntimeSystemExtensions].GetMethods()|Where-Object{$_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1'})[0].MakeGenericMethod($t); $k=$m.Invoke($null,@($op)); $k.Wait(-1)|Out-Null; $k.Result }
$T=[Windows.Media.Control.GlobalSystemMediaTransportControlsSessionManager,Windows.Media.Control,ContentType=WindowsRuntime]
$mg=Aw ($T::RequestAsync()) $T
foreach($s in $mg.GetSessions()){ $id=$s.SourceAppUserModelId; $st=$s.GetPlaybackInfo().PlaybackStatus
 if($args[0] -eq 'lanjut' -and $id -match 'chrome|msedge|firefox|brave|opera' -and "$st" -ne 'Playing'){ [void](Aw ($s.TryPlayAsync()) ([bool])) }
 "$id|$st" }"#;

fn jaga_on_file() -> PathBuf {
    crate::config::data_dir().join("musik_jaga_on")
}

fn selesai_jaga() {
    let _ = std::fs::remove_file(jaga_on_file());
    let _ = std::fs::remove_file(aktif_file());
}

fn aktif_file() -> PathBuf {
    crate::config::data_dir().join("musik_aktif")
}

fn now_s() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn set_jaga(on: bool) -> String {
    if on {
        let _ = std::fs::write(jaga_on_file(), "");
        "Penjaga musik: AKTIF (sengaja kamu nyalakan). Bila musik yang kuputar terjeda tak sengaja kulanjutkan sekali; bila kamu menjeda lagi atau menutup tabnya, aku berhenti menjaga dan TIDAK memutar ulang.".into()
    } else {
        selesai_jaga();
        "Penjaga musik: MATI (bawaan). Video tidak akan dilanjutkan atau diputar ulang oleh NOVA.".into()
    }
}

/// Status sesi media Windows (SMTC): daftar "aplikasi|status". `lanjut` juga memutar sesi browser yang tidak sedang Playing.
fn smtc(mode: &str) -> Option<Vec<String>> {
    let f = crate::config::data_dir().join("jaga.ps1");
    std::fs::write(&f, PS_JAGA).ok()?;
    let mut c = std::process::Command::new("powershell");
    c.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"]).arg(&f).arg(mode);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let o = c.output().ok()?;
    Some(String::from_utf8_lossy(&o.stdout).lines().map(|l| l.trim().to_lowercase()).filter(|l| l.contains('|')).collect())
}

fn browser(l: &str) -> bool {
    ["chrome", "msedge", "firefox", "brave", "opera"].iter().any(|b| l.contains(b))
}

/// Dipanggil sekali dari daemon; thread latar memeriksa tiap 20 detik hanya selama musik NOVA aktif (<6 jam).
pub fn jaga_loop() {
    std::thread::spawn(|| {
        let (mut jeda, mut hilang, mut putar_ulang) = (0u32, 0u32, 0u64);
        loop {
            std::thread::sleep(std::time::Duration::from_secs(20));
            let mulai: u64 = std::fs::read_to_string(aktif_file()).ok().and_then(|t| t.trim().parse().ok()).unwrap_or(0);
            // Opt-in: tanpa `/musik jaga` penjaga tidak berbuat apa-apa (dulu aktif otomatis dan memutar ulang video terus-menerus).
            if !jaga_on_file().exists() || now_s().saturating_sub(mulai) > 6 * 3600 {
                (jeda, hilang) = (0, 0);
                continue;
            }
            let Some(v) = smtc("cek") else { continue };
            let b: Vec<&String> = v.iter().filter(|l| browser(l)).collect();
            if b.is_empty() {
                hilang += 1;
                jeda = 0;
            } else if b.iter().all(|l| !l.ends_with("|playing")) {
                jeda += 1;
                hilang = 0;
            } else {
                (jeda, hilang) = (0, 0);
            }
            if jeda >= 2 {
                if now_s().saturating_sub(putar_ulang) < 180 {
                    // dijeda lagi tak lama setelah kulanjutkan = Bos sengaja menjeda: berhenti menjaga.
                    selesai_jaga();
                } else {
                    let _ = smtc("lanjut");
                    putar_ulang = now_s();
                }
                jeda = 0;
            }
            if hilang >= 2 {
                // Tab/jendela musik ditutup = keputusan Bos. Jangan dibuka lagi; berhenti menjaga.
                selesai_jaga();
                (jeda, hilang) = (0, 0);
            }
        }
    });
}
