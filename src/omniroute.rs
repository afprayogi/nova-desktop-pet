//! OmniRoute: penyedia model kompatibel OpenAI (router/gateway seperti OmniRoute, LiteLLM, OpenRouter, dll.).
//! Bos mengatur URL + API key (kunci dienkripsi DPAPI). NOVA memindai `GET /models`, memberi skor tiap model (keluarga, versi,
//! ukuran, penanda latest/preview), lalu MENGUJI kandidat teratas dengan satu panggilan kecil (keberhasilan + latensi) dan memilih
//! yang terbaik. Saat dipakai, urutan hasil ranking dicoba satu per satu dengan jeda untuk model yang gagal.
use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Serialize, Deserialize, Clone, Default)]
struct Cfg {
    url: String,
    key_enc: String,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Ranked {
    pub model: String,
    pub score: f32,
    pub ms: u64,
}

fn cfg_path() -> std::path::PathBuf {
    crate::config::data_dir().join("omniroute.json")
}

fn rank_path() -> std::path::PathBuf {
    crate::config::data_dir().join("omniroute_models.json")
}

fn cfg() -> Cfg {
    std::fs::read_to_string(cfg_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn key() -> String {
    let c = cfg();
    if c.key_enc.is_empty() { String::new() } else { crate::secure::unprotect(&c.key_enc).unwrap_or_default() }
}

fn base() -> String {
    cfg().url.trim().trim_end_matches('/').to_string()
}

pub fn ranked() -> Vec<Ranked> {
    std::fs::read_to_string(rank_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// URL dan model terpilih tersedia?
pub fn usable() -> bool {
    !base().is_empty() && !ranked().is_empty()
}

pub fn status() -> Value {
    let c = cfg();
    json!({"url": c.url, "has_key": !c.key_enc.is_empty(), "models": ranked()})
}

pub fn save(url: &str, key: Option<&str>) -> Result<()> {
    let url = url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        bail!("URL harus diawali http:// atau https://");
    }
    let mut c = cfg();
    c.url = url.to_string();
    if let Some(k) = key.filter(|k| !k.trim().is_empty()) {
        c.key_enc = crate::secure::protect(k.trim()).ok_or_else(|| anyhow!("gagal mengenkripsi key"))?;
    }
    std::fs::write(cfg_path(), serde_json::to_string(&c)?)?;
    let _ = std::fs::remove_file(rank_path());
    Ok(())
}

pub fn clear() {
    let _ = std::fs::remove_file(cfg_path());
    let _ = std::fs::remove_file(rank_path());
}

// ---------------- pemeringkatan model ----------------

/// Skor kualitas perkiraan dari nama model (tanpa memanggil model): makin tinggi makin diutamakan.
pub fn score(id: &str) -> Option<f32> {
    let l = id.to_lowercase();
    let bad = ["embed", "whisper", "tts", "audio", "image", "dall", "moderation", "rerank", "vision-only", "stable-diffusion", "sdxl", "flux", "speech", "transcribe", "realtime", "guard", "ocr"];
    if bad.iter().any(|b| l.contains(b)) {
        return None;
    }
    let mut s = 10.0f32;
    // versi: angka berformat 3, 3.5, 4o, 5 ...
    let nums: Vec<f32> = l.split(|c: char| !(c.is_ascii_digit() || c == '.')).filter_map(|t| t.trim_matches('.').parse::<f32>().ok()).filter(|n| *n < 20.0).collect();
    if let Some(v) = nums.first() {
        s += v * 4.0;
    }
    // ukuran parameter: 70b, 405b, 8b
    if let Some(b) = l.split(|c: char| !(c.is_ascii_digit() || c == '.' || c == 'b')).find_map(|t| t.strip_suffix('b').and_then(|n| n.parse::<f32>().ok())) {
        s += (b.max(1.0)).ln() * 3.0;
    }
    for (k, w) in [("opus", 14.0), ("gpt-5", 14.0), ("o3", 12.0), ("o1", 8.0), ("sonnet", 10.0), ("pro", 8.0), ("large", 6.0), ("ultra", 10.0), ("max", 8.0), ("405b", 6.0), ("70b", 3.0), ("gpt-4", 6.0), ("gemini", 3.0), ("deepseek", 5.0), ("qwen", 3.0), ("llama", 2.0), ("latest", 4.0), ("flash", 2.0), ("haiku", -2.0), ("mini", -4.0), ("small", -4.0), ("lite", -6.0), ("nano", -9.0), ("tiny", -9.0), ("preview", -3.0), ("exp", -3.0), ("beta", -2.0), ("coder", -2.0), ("thinking", -1.0)] {
        if l.contains(k) {
            s += w;
        }
    }
    Some(s)
}

async fn call_model(http: &reqwest::Client, model: &str, system: &str, user: &str, json_out: bool, max_tokens: u32) -> Result<String> {
    let mut body = json!({"model": model, "temperature": 0.3, "max_tokens": max_tokens, "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}]});
    if json_out {
        body["response_format"] = json!({"type": "json_object"});
    }
    let k = key();
    let mut rq = http.post(format!("{}/chat/completions", base())).timeout(Duration::from_secs(60)).json(&body);
    if !k.is_empty() {
        rq = rq.bearer_auth(k);
    }
    let resp = rq.send().await?;
    let st = resp.status();
    let v: Value = resp.json().await.unwrap_or(Value::Null);
    if !st.is_success() {
        bail!("HTTP {}: {}", st.as_u16(), v["error"]["message"].as_str().unwrap_or("").chars().take(120).collect::<String>());
    }
    crate::makan::feed(v["usage"]["total_tokens"].as_u64().unwrap_or(0));
    let t = v["choices"][0]["message"]["content"].as_str().unwrap_or("").trim().to_string();
    if t.is_empty() {
        bail!("jawaban kosong");
    }
    Ok(t)
}

/// Pindai model → beri skor → uji kandidat teratas → simpan peringkat. Mengembalikan peringkat akhir.
pub async fn scan(http: &reqwest::Client) -> Result<Vec<Ranked>> {
    let b = base();
    if b.is_empty() {
        bail!("URL OmniRoute belum diatur");
    }
    let k = key();
    let mut rq = http.get(format!("{b}/models")).timeout(Duration::from_secs(20));
    if !k.is_empty() {
        rq = rq.bearer_auth(k);
    }
    let v: Value = rq.send().await?.json().await.map_err(|e| anyhow!("balasan /models tidak terbaca: {e}"))?;
    if let Some(m) = v["error"]["message"].as_str() {
        bail!("{m}");
    }
    let ids: Vec<String> = v["data"].as_array().or_else(|| v["models"].as_array()).map(|a| a.iter().filter_map(|m| m["id"].as_str().or_else(|| m["name"].as_str()).map(String::from)).collect()).unwrap_or_default();
    if ids.is_empty() {
        bail!("tidak ada model yang dikembalikan oleh {b}/models");
    }
    let mut cands: Vec<(String, f32)> = ids.iter().filter_map(|i| score(i).map(|s| (i.clone(), s))).collect();
    cands.sort_by(|a, b| b.1.total_cmp(&a.1));
    // uji 8 kandidat teratas: gagal dibuang, latensi memengaruhi skor akhir
    let mut out: Vec<Ranked> = vec![];
    for (m, s) in cands.iter().take(8) {
        let t0 = Instant::now();
        if call_model(http, m, "Balas satu kata.", "Katakan: ok", false, 8).await.is_ok() {
            let ms = t0.elapsed().as_millis() as u64;
            out.push(Ranked { model: m.clone(), score: s - (ms as f32 / 1500.0), ms });
        }
    }
    if out.is_empty() {
        bail!("tidak ada model teratas yang berhasil diuji (periksa key/kuota). Model tersedia: {}", ids.iter().take(8).cloned().collect::<Vec<_>>().join(", "));
    }
    out.sort_by(|a, b| b.score.total_cmp(&a.score));
    // sisa kandidat tak teruji dilampirkan sebagai cadangan (latensi tak diketahui)
    for (m, s) in cands.iter().skip(8).take(6) {
        out.push(Ranked { model: m.clone(), score: s - 20.0, ms: 0 });
    }
    std::fs::write(rank_path(), serde_json::to_string(&out)?)?;
    Ok(out)
}

static COOL: Mutex<Vec<(String, Instant)>> = Mutex::new(Vec::new());

/// Panggilan teks via OmniRoute: mencoba model sesuai peringkat; yang gagal dijeda 10 menit.
pub async fn chat(http: &reqwest::Client, system: &str, user: &str, json_out: bool) -> Result<String> {
    if base().is_empty() {
        bail!("OmniRoute belum diatur");
    }
    let mut list = ranked();
    if list.is_empty() {
        list = scan(http).await?;
    }
    let mut last = String::new();
    for r in list.iter().take(5) {
        if COOL.lock().map(|c| c.iter().any(|(m, t)| *m == r.model && t.elapsed() < Duration::from_secs(600))).unwrap_or(false) {
            continue;
        }
        match call_model(http, &r.model, system, user, json_out, 900).await {
            Ok(t) => return Ok(t),
            Err(e) => {
                last = format!("{}: {e}", r.model);
                if let Ok(mut c) = COOL.lock() {
                    c.retain(|(m, _)| *m != r.model);
                    c.push((r.model.clone(), Instant::now()));
                }
            }
        }
    }
    bail!("semua model OmniRoute gagal ({last})")
}

pub fn laporan() -> String {
    let c = cfg();
    if c.url.is_empty() {
        return "OmniRoute belum diatur. Buka menu Pengaturan → OmniRoute (URL + API key) di antarmuka web, atau /omni url <alamat>.".into();
    }
    let r = ranked();
    format!("OmniRoute: {} · key {} · {} model teruji{}", c.url, if c.key_enc.is_empty() { "tidak ada" } else { "tersimpan (terenkripsi)" }, r.len(), r.first().map(|m| format!(" · terbaik: {} ({} ms)", m.model, m.ms)).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ranking_heuristics() {
        assert!(score("text-embedding-3-large").is_none() && score("whisper-1").is_none());
        assert!(score("claude-opus-4").unwrap() > score("claude-haiku-3").unwrap());
        assert!(score("llama-3.3-70b").unwrap() > score("llama-3.2-1b").unwrap());
        assert!(score("gpt-5").unwrap() > score("gpt-4o-mini").unwrap());
        assert!(score("gemini-pro-latest").unwrap() > score("gemini-flash-lite-preview").unwrap());
    }
}
