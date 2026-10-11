//! Pemantau kesehatan laptop. Hemat RAM/CPU: sampel jarang, riwayat disimpan di disk (SQLite).
use crate::adblock;
use crate::config;
use crate::engine::{Event, Tx};
use rusqlite::Connection;
use serde::Serialize;
use std::sync::Mutex;
use std::sync::atomic::Ordering;
use std::time::Duration;
use sysinfo::{Disks, ProcessesToUpdate, System};

#[derive(Clone, Serialize, Default)]
pub struct ProcRow {
    pub name: String,
    pub cpu: f32,
    pub mem_mb: u64,
}

#[derive(Clone, Serialize, Default)]
pub struct Health {
    pub cpu: f32,
    pub ram_used_mb: u64,
    pub ram_total_mb: u64,
    pub disk_used_gb: u64,
    pub disk_total_gb: u64,
    pub uptime_min: u64,
    pub procs: Vec<ProcRow>,
    /// Riwayat CPU terakhir (dibaca dari disk) untuk grafik mini.
    pub history: Vec<f32>,
    pub ads_on: bool,
    pub ads_blocked: u64,
    pub ads_total: u64,
    pub ads_listed: u64,
}

static LAST: Mutex<String> = Mutex::new(String::new());

/// Ringkasan teks untuk alat `health` (dipakai agen).
pub fn summary() -> String {
    let s = LAST.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if s.is_empty() { "belum ada data kesehatan".into() } else { s }
}

const INTERVAL: Duration = Duration::from_secs(4);

pub async fn run(tx: Tx) {
    let db = Connection::open(config::data_dir().join("kesehatan.db")).ok();
    if let Some(c) = &db {
        let _ = c.execute_batch("CREATE TABLE IF NOT EXISTS h(ts INTEGER, cpu REAL, ram REAL);");
    }
    let mut sys = System::new();
    let mut disks = Disks::new_with_refreshed_list();
    let (mut hot_cpu, mut tick, mut cool) = (0u32, 0u64, 0u32);
    loop {
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        // Daftar proses lebih mahal: segarkan tiap 2 siklus saja.
        if tick % 2 == 0 {
            sys.refresh_processes(ProcessesToUpdate::All, true);
            disks.refresh();
        }
        let mut procs: Vec<ProcRow> = sys
            .processes()
            .values()
            .map(|p| ProcRow {
                name: p.name().to_string_lossy().into_owned(),
                cpu: p.cpu_usage(),
                mem_mb: p.memory() / 1_048_576,
            })
            .collect();
        procs.sort_by(|a, b| (b.cpu * 4.0 + b.mem_mb as f32 / 50.0).total_cmp(&(a.cpu * 4.0 + a.mem_mb as f32 / 50.0)));
        procs.truncate(6);

        let cpu = sys.global_cpu_usage();
        let (ram_used, ram_total) = (sys.used_memory() / 1_048_576, sys.total_memory() / 1_048_576);
        let (mut du, mut dt) = (0u64, 0u64);
        for d in disks.list() {
            dt += d.total_space() / 1_073_741_824;
            du += (d.total_space() - d.available_space()) / 1_073_741_824;
        }

        let mut history = Vec::new();
        if let Some(c) = &db {
            let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            let ramp = if ram_total > 0 { ram_used as f64 * 100.0 / ram_total as f64 } else { 0.0 };
            let _ = c.execute("INSERT INTO h VALUES(?1,?2,?3)", (ts as i64, cpu as f64, ramp));
            if tick % 50 == 0 {
                let _ = c.execute("DELETE FROM h WHERE ts < ?1", (ts as i64 - 86_400,));
            }
            if let Ok(mut st) = c.prepare("SELECT cpu FROM h ORDER BY ts DESC LIMIT 40") {
                if let Ok(rows) = st.query_map([], |r| r.get::<_, f64>(0)) {
                    history = rows.flatten().map(|v| v as f32).collect();
                    history.reverse();
                }
            }
        }

        let h = Health {
            cpu,
            ram_used_mb: ram_used,
            ram_total_mb: ram_total,
            disk_used_gb: du,
            disk_total_gb: dt,
            uptime_min: System::uptime() / 60,
            procs,
            history,
            ads_on: adblock::ON.load(Ordering::Relaxed),
            ads_blocked: adblock::BLOCKED.load(Ordering::Relaxed),
            ads_total: adblock::TOTAL.load(Ordering::Relaxed),
            ads_listed: adblock::LISTED.load(Ordering::Relaxed),
        };
        *LAST.lock().unwrap_or_else(|e| e.into_inner()) = format!(
            "CPU {:.0}% | RAM {}/{} MB | Disk {}/{} GB | Uptime {} mnt | Proses teratas: {}",
            h.cpu, h.ram_used_mb, h.ram_total_mb, h.disk_used_gb, h.disk_total_gb, h.uptime_min,
            h.procs.iter().map(|p| format!("{} ({:.0}% / {}MB)", p.name, p.cpu, p.mem_mb)).collect::<Vec<_>>().join(", ")
        );

        // Peringatan dengan jeda agar tidak membanjiri.
        hot_cpu = if cpu > 90.0 { hot_cpu + 1 } else { 0 };
        cool = cool.saturating_sub(1);
        if cool == 0 {
            let top = h.procs.first().map(|p| p.name.clone()).unwrap_or_default();
            let alert = if hot_cpu >= 3 {
                Some(format!("⚠ CPU tinggi ({cpu:.0}%) terus-menerus. Terberat: {top}"))
            } else if ram_total > 0 && ram_used * 100 / ram_total >= 88 {
                Some(format!("⚠ RAM hampir penuh ({ram_used}/{ram_total} MB). Terberat: {top}"))
            } else if dt > 0 && du * 100 / dt >= 92 {
                Some(format!("⚠ Disk hampir penuh ({du}/{dt} GB)"))
            } else {
                None
            };
            // jeda per jenis: disk 6 jam, lainnya 30 menit (tidak mengulang tiap menit)
            let alert = alert.filter(|a| {
                static LAST: Mutex<Vec<(&'static str, std::time::Instant)>> = Mutex::new(Vec::new());
                let (kind, gap) = if a.contains("Disk") { ("disk", 6 * 3600) } else if a.contains("RAM") { ("ram", 1800) } else { ("cpu", 1800) };
                let mut g = LAST.lock().unwrap_or_else(|e| e.into_inner());
                if g.iter().any(|(k, t)| *k == kind && t.elapsed().as_secs() < gap) {
                    return false;
                }
                g.retain(|(k, _)| *k != kind);
                g.push((kind, std::time::Instant::now()));
                true
            });
            if let Some(a) = alert {
                crate::notify::toast("NOVA - peringatan", &a);
                crate::adegan::on_alert(&tx, &a, &top);
                if a.contains("Disk") {
                    crate::saran::offer_always(&tx, "Disk hampir penuh. Bersihkan cache aman (Temp, npm, pip, Hugging Face)?", "bersihkan cache aman", "disk");
                }
                if a.contains("RAM") {
                    crate::saran::offer(&tx, "RAM hampir penuh. Mau kututup aplikasi yang tidak dipakai?", "matikan aplikasi yang tidak dipakai", "ram");
                }
                cool = 15;
            }
        }
        tx.send(Event::Health(Box::new(h)));
        tick += 1;
        tokio::time::sleep(INTERVAL).await;
    }
}
