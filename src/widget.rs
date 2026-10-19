//! Widget dinamis & mekanik Tamagotchi: To-Do harian, fokus (Pomodoro) dengan hadiah koin, kontrol musik (tombol media Windows,
//! judul lagu Spotify), pemantau sistem (CPU/RAM/disk + suhu GPU NVIDIA bila ada), statistik vital (Energi, Fokus, Senang),
//! dompet (koin, makanan) dan toko topi. Semua lewat satu titik: `aksi()` dan `data()` (API /api/widget).
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Serialize, Deserialize, Clone, Default)]
struct D {
    koin: i64,
    makanan: i64,
    topi_dimiliki: Vec<String>,
    topi_pakai: String,
    fokus_menit: std::collections::HashMap<String, u32>,
    fokus_sesi: u32,
}

static ST: Mutex<Option<D>> = Mutex::new(None);
static FOKUS: Mutex<Option<(Instant, u32)>> = Mutex::new(None); // (selesai pada, menit)
static CACHE: Mutex<Option<(Instant, Value)>> = Mutex::new(None);

pub const TOPI: [(&str, i64); 5] = [("kupluk", 20), ("baling-baling", 25), ("mahkota", 30), ("penyihir", 40), ("halo", 50)];
const HARGA_MAKAN: i64 = 5;

fn path() -> std::path::PathBuf {
    crate::config::data_dir().join("dompet.json")
}

fn with<R>(f: impl FnOnce(&mut D) -> R) -> R {
    let mut g = ST.lock().unwrap_or_else(|e| e.into_inner());
    let d = g.get_or_insert_with(|| std::fs::read_to_string(path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| D { koin: 10, makanan: 1, ..Default::default() }));
    let before = serde_json::to_string(d).unwrap_or_default();
    let r = f(d);
    let after = serde_json::to_string(d).unwrap_or_default();
    if after != before {
        let _ = std::fs::write(path(), after);
    }
    r
}

fn hari() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

pub fn tambah_koin(n: i64) {
    with(|d| d.koin += n);
}

pub fn topi() -> String {
    with(|d| d.topi_pakai.clone())
}

// ---------------- to-do ----------------

fn tdb() -> Option<rusqlite::Connection> {
    let c = crate::assistant::db().ok()?;
    c.execute_batch("CREATE TABLE IF NOT EXISTS todo(id INTEGER PRIMARY KEY, teks TEXT, selesai INT DEFAULT 0, hari TEXT);").ok()?;
    Some(c)
}

/// (menit fokus hari ini, total sesi) untuk rekap kerja.
pub fn fokus_hari_ini() -> (u32, u32) {
    with(|d| (d.fokus_menit.get(&hari()).copied().unwrap_or(0), d.fokus_sesi))
}

pub(crate) fn todo_list() -> Vec<Value> {
    let Some(c) = tdb() else { return vec![] };
    // daftar harian: yang belum selesai (dari hari sebelumnya ikut terbawa) + yang selesai hari ini
    let Ok(mut st) = c.prepare("SELECT id, teks, selesai FROM todo WHERE selesai=0 OR hari=?1 ORDER BY selesai, id") else { return vec![] };
    st.query_map([hari()], |r| Ok(json!({"id": r.get::<_, i64>(0)?, "teks": r.get::<_, String>(1)?, "selesai": r.get::<_, i64>(2)? == 1}))).map(|r| r.flatten().collect()).unwrap_or_default()
}

// ---------------- musik ----------------

#[cfg(windows)]
fn media_key(vk: u8) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{keybd_event, KEYEVENTF_KEYUP};
    unsafe {
        keybd_event(vk, 0, 0, 0);
        keybd_event(vk, 0, KEYEVENTF_KEYUP, 0);
    }
}

#[cfg(not(windows))]
fn media_key(_vk: u8) {}

fn ps(cmd: &str) -> String {
    let mut c = std::process::Command::new("powershell");
    c.args(["-NoProfile", "-Command", cmd]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    c.output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default()
}

fn musik_judul() -> String {
    let t = ps("(Get-Process Spotify -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowTitle } | Select-Object -First 1).MainWindowTitle");
    if t.is_empty() { String::new() } else if t.starts_with("Spotify") { "Dijeda (Spotify)".into() } else { t }
}

fn gpu_suhu() -> String {
    let mut c = std::process::Command::new("nvidia-smi");
    c.args(["--query-gpu=temperature.gpu", "--format=csv,noheader,nounits"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    c.output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_default()
}

// ---------------- vital ----------------

/// Hasil mahal (pola 14 hari, Spotify, nvidia-smi) disimpan sementara agar polling 3 detik tidak membebani CPU.
fn cached<T: Clone + Send + 'static>(slot: &'static Mutex<Option<(Instant, T)>>, ttl: u64, f: impl FnOnce() -> T) -> T {
    if let Some((t, v)) = slot.lock().ok().and_then(|g| g.clone()) {
        if t.elapsed() < Duration::from_secs(ttl) {
            return v;
        }
    }
    let v = f();
    if let Ok(mut g) = slot.lock() {
        *g = Some((Instant::now(), v.clone()));
    }
    v
}

static VITAL_C: Mutex<Option<(Instant, (f32, f32, f32))>> = Mutex::new(None);
static MEDIA_C: Mutex<Option<(Instant, (String, String))>> = Mutex::new(None);

fn vital() -> (f32, f32, f32) {
    cached(&VITAL_C, 30, vital_raw)
}

fn vital_raw() -> (f32, f32, f32) {
    let p = crate::hewan::pola();
    let streak = crate::assistant::streak_min();
    let energi = (100.0 - crate::hewan::kelelahan(&p, streak) * 100.0).clamp(0.0, 100.0);
    let menit = with(|d| *d.fokus_menit.get(&hari()).unwrap_or(&0)) as f32;
    let fokus = (10.0 + menit).min(100.0);
    let b = crate::buddy::get();
    let mood = match crate::buddy::mood(&b) { "puas" => 100.0, "senang" => 80.0, "penasaran" => 60.0, "netral" => 45.0, _ => 20.0 };
    let senang = 0.5 * crate::makan::kenyang() + 0.5 * mood;
    (energi, fokus, senang)
}

// ---------------- API ----------------

/// Semua data widget (di-cache 3 detik: musik/GPU memanggil proses).
pub fn data() -> Value {
    if let Some((t, v)) = CACHE.lock().ok().and_then(|g| g.clone()) {
        if t.elapsed() < Duration::from_secs(3) {
            return v;
        }
    }
    let (e, f, s) = vital();
    let fokus = FOKUS.lock().ok().and_then(|g| *g).map(|(t, m)| json!({"sisa": t.saturating_duration_since(Instant::now()).as_secs(), "menit": m}));
    let v = with(|d| {
        json!({
            "vital": {"energi": e, "fokus": f, "senang": s, "kenyang": crate::makan::kenyang()},
            "koin": d.koin, "makanan": d.makanan, "topi_pakai": d.topi_pakai, "topi_dimiliki": d.topi_dimiliki,
            "toko": TOPI.iter().map(|(n, h)| json!({"nama": n, "harga": h})).collect::<Vec<_>>(), "harga_makan": HARGA_MAKAN,
            "fokus_hari": d.fokus_menit.get(&hari()).copied().unwrap_or(0), "fokus_sesi": d.fokus_sesi,
        })
    });
    let mut v = v;
    v["fokus"] = fokus.unwrap_or(Value::Null);
    v["todo"] = json!(todo_list());
    let (musik, gpu) = cached(&MEDIA_C, 12, || (musik_judul(), gpu_suhu()));
    v["musik"] = json!(musik);
    v["gpu"] = json!(gpu);
    if let Ok(mut g) = CACHE.lock() {
        *g = Some((Instant::now(), v.clone()));
    }
    v
}

/// Jalankan aksi dari UI. Mengembalikan pesan singkat.
pub fn aksi(a: &Value) -> String {
    if let Ok(mut g) = CACHE.lock() {
        *g = None;
    }
    let s = |k: &str| a[k].as_str().unwrap_or("").trim().to_string();
    match s("aksi").as_str() {
        "todo_tambah" => {
            let t = s("teks");
            if t.is_empty() {
                return "Tulis tugasnya dulu.".into();
            }
            if let Some(c) = tdb() {
                let _ = c.execute("INSERT INTO todo(teks, hari) VALUES(?1, ?2)", rusqlite::params![t, hari()]);
            }
            "Ditambahkan.".into()
        }
        "todo_selesai" => {
            let id = a["id"].as_i64().unwrap_or(0);
            let Some(c) = tdb() else { return String::new() };
            let was: i64 = c.query_row("SELECT selesai FROM todo WHERE id=?1", [id], |r| r.get(0)).unwrap_or(1);
            let _ = c.execute("UPDATE todo SET selesai=?2, hari=?3 WHERE id=?1", rusqlite::params![id, 1 - was, hari()]);
            if was == 0 {
                tambah_koin(2);
                crate::makan::catat("todo");
                return "Selesai! +2 koin 🪙".into();
            }
            "Dibuka lagi.".into()
        }
        "todo_hapus" => {
            if let Some(c) = tdb() {
                let _ = c.execute("DELETE FROM todo WHERE id=?1", [a["id"].as_i64().unwrap_or(0)]);
            }
            "Dihapus.".into()
        }
        "fokus_mulai" => {
            let m = a["menit"].as_u64().unwrap_or(25).clamp(5, 90) as u32;
            if let Ok(mut g) = FOKUS.lock() {
                *g = Some((Instant::now() + Duration::from_secs(m as u64 * 60), m));
            }
            format!("Fokus {m} menit dimulai. Semangat! 🎯")
        }
        "fokus_stop" => {
            if let Ok(mut g) = FOKUS.lock() {
                *g = None;
            }
            "Sesi fokus dihentikan (tanpa hadiah).".into()
        }
        "musik" => {
            match s("tombol").as_str() {
                "next" => media_key(0xB0),
                "prev" => media_key(0xB1),
                _ => media_key(0xB3),
            }
            "OK".into()
        }
        "beri_makan" => {
            let ok = with(|d| {
                if d.makanan > 0 {
                    d.makanan -= 1;
                    true
                } else {
                    false
                }
            });
            if ok {
                crate::makan::beri_makan(30.0);
                "Nyam! 🍖".into()
            } else {
                "Tidak ada makanan — beli di toko.".into()
            }
        }
        "beli" => {
            let item = s("item");
            with(|d| {
                if item == "makanan" {
                    if d.koin >= HARGA_MAKAN {
                        d.koin -= HARGA_MAKAN;
                        d.makanan += 1;
                        return format!("Makanan dibeli (-{HARGA_MAKAN} koin).");
                    }
                    return "Koin tidak cukup.".into();
                }
                let Some((_, h)) = TOPI.iter().find(|(n, _)| *n == item) else { return "Barang tidak ada.".into() };
                if d.topi_dimiliki.contains(&item) {
                    return "Sudah dimiliki.".into();
                }
                if d.koin < *h {
                    return format!("Koin kurang: butuh {h}, punya {}.", d.koin);
                }
                d.koin -= h;
                d.topi_dimiliki.push(item.clone());
                d.topi_pakai = item.clone();
                format!("Topi '{item}' dibeli dan dipakai! 🎩")
            })
        }
        "pakai" => with(|d| {
            let item = s("item");
            if item.is_empty() || d.topi_dimiliki.contains(&item) {
                d.topi_pakai = item;
                "Tampilan diperbarui.".into()
            } else {
                "Belum dimiliki.".to_string()
            }
        }),
        _ => "aksi tidak dikenal".into(),
    }
}

/// Latar: selesaikan sesi fokus dan beri hadiah.
pub fn start(tx: crate::engine::Tx) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(2));
        let done = FOKUS.lock().ok().and_then(|mut g| match *g {
            Some((t, m)) if Instant::now() >= t => {
                *g = None;
                Some(m)
            }
            _ => None,
        });
        if let Some(m) = done {
            let koin = 8 + (m as i64 / 5);
            with(|d| {
                d.koin += koin;
                d.fokus_sesi += 1;
                *d.fokus_menit.entry(hari()).or_default() += m;
            });
            crate::makan::catat("fokus");
            let lv = crate::buddy::add_xp_amt(10 + m as u64);
            let msg = format!("🎯 Sesi fokus {m} menit selesai! +{koin} koin 🪙 dan XP. Istirahat sebentar ya.");
            crate::notify::toast("NOVA - fokus selesai", &msg);
            tx.notice(msg);
            if let Some(l) = lv {
                tx.notice(l);
            }
            tx.send(crate::engine::Event::Mood { emo: "gembira".into(), level: 1.0 });
            crate::makan::sfx("achievement");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shop_and_todo() {
        with(|d| *d = D { koin: 100, makanan: 0, ..Default::default() });
        assert!(aksi(&json!({"aksi": "beli", "item": "kupluk"})).contains("dibeli"));
        assert!(aksi(&json!({"aksi": "beli", "item": "kupluk"})).contains("Sudah"));
        assert_eq!(topi(), "kupluk");
        assert!(aksi(&json!({"aksi": "beli", "item": "makanan"})).contains("dibeli"));
        let before = with(|d| d.koin);
        aksi(&json!({"aksi": "todo_tambah", "teks": "uji widget"}));
        let id = todo_list().iter().find(|t| t["teks"] == "uji widget").and_then(|t| t["id"].as_i64()).unwrap();
        assert!(aksi(&json!({"aksi": "todo_selesai", "id": id})).contains("+2"));
        assert_eq!(with(|d| d.koin), before + 2);
        aksi(&json!({"aksi": "todo_hapus", "id": id}));
    }
}
