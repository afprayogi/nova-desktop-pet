//! Llama/Ollama lokal yang RINGAN dan bisa dinyalakan/dimatikan: model kecil (mis. llama3.2:1b), konteks pendek, thread CPU dibatasi,
//! satu model pada satu waktu dan otomatis dibongkar dari RAM setelah 2 menit tidak dipakai (keep_alive). `/llm on` menyalakan,
//! `/llm off` membongkar model dan menghentikan server yang NOVA nyalakan sendiri. Memakai Ollama yang sudah terpasang di laptop.
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

pub const URL: &str = "http://127.0.0.1:11434";
pub const SUGGEST: &[(&str, &str)] = &[("llama3.2:1b", "±1,3 GB, paling ringan (disarankan untuk RAM 8 GB)"), ("llama3.2:3b", "±2 GB, lebih pintar tapi lebih berat"), ("qwen2.5:1.5b", "±1 GB, ringan, bagus berbahasa campuran")];

fn flag() -> PathBuf {
    crate::config::data_dir().join("llm.on")
}

pub fn enabled() -> bool {
    flag().exists()
}

pub fn set_enabled(on: bool) {
    if on {
        let _ = std::fs::write(flag(), "1");
    } else {
        let _ = std::fs::remove_file(flag());
    }
}

/// Model pilihan Bos (llm.model) atau None.
pub fn preferred() -> Option<String> {
    std::fs::read_to_string(crate::config::data_dir().join("llm.model")).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

pub fn set_preferred(m: &str) {
    let _ = std::fs::write(crate::config::data_dir().join("llm.model"), m);
}

/// Thread CPU yang dipakai model: setengah inti (1-4) agar laptop tetap responsif.
pub fn threads() -> usize {
    (std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4) / 2).clamp(1, 4)
}

pub fn ollama_exe() -> Option<PathBuf> {
    let mut c: Vec<PathBuf> = vec![];
    if let Some(paths) = std::env::var_os("PATH") {
        c.extend(std::env::split_paths(&paths).map(|d| d.join("ollama.exe")));
    }
    if let Some(l) = dirs::data_local_dir() {
        c.push(l.join("Programs").join("Ollama").join("ollama.exe"));
    }
    c.push(PathBuf::from(r"C:\Program Files\Ollama\ollama.exe"));
    c.push(PathBuf::from(r"D:\Ollama\ollama.exe"));
    c.into_iter().find(|p| p.is_file())
}

pub async fn running(http: &reqwest::Client) -> bool {
    http.get(format!("{URL}/api/tags")).timeout(Duration::from_secs(3)).send().await.map(|r| r.status().is_success()).unwrap_or(false)
}

/// Jalankan `ollama serve` hemat sumber daya (prioritas di bawah normal; anak prosesnya mewarisi).
pub fn start_serve() -> Option<std::process::Child> {
    let exe = ollama_exe()?;
    let mut c = Command::new(exe);
    c.arg("serve").env("OLLAMA_KEEP_ALIVE", "2m").env("OLLAMA_MAX_LOADED_MODELS", "1").env("OLLAMA_NUM_PARALLEL", "1").stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000 | 0x0000_4000); // tanpa jendela + BELOW_NORMAL_PRIORITY_CLASS
    }
    c.spawn().ok()
}

pub async fn installed(http: &reqwest::Client) -> Vec<String> {
    let Ok(r) = http.get(format!("{URL}/api/tags")).send().await else { return vec![] };
    r.json::<serde_json::Value>().await.ok().and_then(|v| v["models"].as_array().map(|a| a.iter().filter_map(|m| m["name"].as_str().map(String::from)).collect())).unwrap_or_default()
}

pub async fn loaded(http: &reqwest::Client) -> Vec<(String, u64)> {
    let Ok(r) = http.get(format!("{URL}/api/ps")).send().await else { return vec![] };
    r.json::<serde_json::Value>().await.ok().and_then(|v| v["models"].as_array().map(|a| a.iter().filter_map(|m| Some((m["name"].as_str()?.to_string(), m["size"].as_u64().unwrap_or(0) / 1_048_576))).collect())).unwrap_or_default()
}

/// Bongkar semua model dari RAM sekarang juga.
pub async fn unload_all(http: &reqwest::Client) -> usize {
    let l = loaded(http).await;
    for (m, _) in &l {
        let _ = http.post(format!("{URL}/api/generate")).json(&serde_json::json!({"model": m, "keep_alive": 0})).send().await;
    }
    l.len()
}

pub fn free_ram_mb() -> u64 {
    let mut s = sysinfo::System::new();
    s.refresh_memory();
    s.available_memory() / 1_048_576
}

/// Pilih model: pilihan Bos, lalu llama kecil, lalu yang terkecil yang terpasang.
pub fn pick(models: &[String]) -> Option<String> {
    pick_with(models, preferred())
}

fn pick_with(models: &[String], pref: Option<String>) -> Option<String> {
    if let Some(p) = pref {
        if models.iter().any(|m| *m == p) {
            return Some(p);
        }
    }
    for want in ["llama3.2:1b", "qwen2.5:1.5b", "llama3.2:3b", "qwen2.5-coder:1.5b"] {
        if let Some(m) = models.iter().find(|m| m.starts_with(want)) {
            return Some(m.clone());
        }
    }
    models.first().cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn picks_small_llama_first() {
        let m = vec!["qwen2.5-coder:1.5b".to_string(), "llama3.2:1b".to_string()];
        assert_eq!(pick_with(&m, None).as_deref(), Some("llama3.2:1b"));
        assert_eq!(pick_with(&["x:7b".to_string()], None).as_deref(), Some("x:7b"));
        assert!((1..=4).contains(&threads()));
    }
}
