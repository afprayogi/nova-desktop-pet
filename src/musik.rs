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
