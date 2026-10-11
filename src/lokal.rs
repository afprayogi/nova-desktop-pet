//! Model lokal (Ollama kedua di port 11435, model disimpan di D:\ollama-models agar tidak memenuhi C:): embedding
//! `nomic-embed-text` untuk pemahaman semantik tanpa kuota, dan `llama3.2:1b` untuk obrolan offline berbahasa Indonesia.
//! Server dijalankan otomatis bila folder model ada; tidak mengubah Ollama utama di port 11434.
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const URL: &str = "http://127.0.0.1:11435";
pub const EMBED_MODEL: &str = "nomic-embed-text";
pub const CHAT_MODEL: &str = "llama3.2:1b";
const MODELS_DIR: &str = "D:/ollama-models";

static CACHE: Mutex<Option<(Instant, Vec<String>)>> = Mutex::new(None);

/// Jalankan server lokal bila belum hidup dan folder model ada.
pub fn ensure_server() {
    if !std::path::Path::new(MODELS_DIR).exists() {
        return;
    }
    let up = std::net::TcpStream::connect_timeout(&"127.0.0.1:11435".parse().unwrap(), Duration::from_millis(400)).is_ok();
    if up {
        return;
    }
    if let Some(exe) = crate::llm::ollama_exe() {
        let mut c = std::process::Command::new(exe);
        c.arg("serve").env("OLLAMA_HOST", "127.0.0.1:11435").env("OLLAMA_MODELS", MODELS_DIR.replace('/', "\\"));
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            c.creation_flags(0x0800_0000);
        }
        let _ = c.spawn();
    }
}

/// Daftar model terpasang (cache 60 detik).
pub async fn models(http: &reqwest::Client) -> Vec<String> {
    if let Some((t, v)) = CACHE.lock().ok().and_then(|g| g.clone()) {
        if t.elapsed() < Duration::from_secs(60) {
            return v;
        }
    }
    let v: Vec<String> = match http.get(format!("{URL}/api/tags")).timeout(Duration::from_secs(2)).send().await {
        Ok(r) => r.json::<Value>().await.ok().and_then(|j| j["models"].as_array().map(|a| a.iter().filter_map(|m| m["name"].as_str().map(String::from)).collect())).unwrap_or_default(),
        Err(_) => vec![],
    };
    if let Ok(mut g) = CACHE.lock() {
        *g = Some((Instant::now(), v.clone()));
    }
    v
}

pub async fn has(http: &reqwest::Client, model: &str) -> bool {
    models(http).await.iter().any(|m| m == model || m.starts_with(&format!("{model}:")))
}

pub async fn embed(http: &reqwest::Client, texts: &[String]) -> Result<Vec<Vec<f32>>> {
    if !has(http, EMBED_MODEL).await {
        bail!("model embedding lokal belum terpasang");
    }
    let v: Value = http.post(format!("{URL}/api/embed")).timeout(Duration::from_secs(60)).json(&json!({"model": EMBED_MODEL, "input": texts, "keep_alive": "2m"})).send().await?.json().await?;
    let out: Vec<Vec<f32>> = v["embeddings"].as_array().ok_or_else(|| anyhow!("balasan embedding kosong"))?.iter().map(|e| e.as_array().map(|x| x.iter().filter_map(|f| f.as_f64().map(|f| f as f32)).collect()).unwrap_or_default()).collect();
    if out.len() != texts.len() {
        bail!("jumlah embedding tidak cocok");
    }
    Ok(out)
}

/// Obrolan offline dengan Llama kecil (gaya NOVA yang ramah).
static HIST: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());
static UNLOAD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Minta model lokal dilepas dari RAM (dijalankan saat Gemini pulih).
pub fn request_unload() {
    UNLOAD.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Dipanggil engine: bila ada permintaan, kosongkan model dari RAM (keep_alive 0).
pub async fn unload_if_requested(http: &reqwest::Client) {
    if !UNLOAD.swap(false, std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    for m in [CHAT_MODEL, EMBED_MODEL] {
        let _ = http.post(format!("{URL}/api/generate")).timeout(Duration::from_secs(5)).json(&json!({"model": m, "keep_alive": 0})).send().await;
    }
}

fn remember(u: &str, a: &str) {
    if let Ok(mut h) = HIST.lock() {
        h.push((u.chars().take(300).collect(), a.chars().take(500).collect()));
        if h.len() > 6 {
            h.remove(0);
        }
    }
}

/// Koding: model koding (qwen2.5-coder di Ollama utama) bila ada, jika tidak Llama lokal.
pub async fn code_chat(http: &reqwest::Client, user: &str) -> Result<String> {
    let sys = "Kamu NOVA, asisten koding yang ramah untuk insinyur listrik/otomasi. Jawab dalam Bahasa Indonesia; kode dalam blok ``` dengan bahasa yang benar; jelaskan singkat (maksimal 5 kalimat di luar kode). Jangan mengarang API; bila ragu, katakan.";
    let main = crate::llm::URL;
    let have: bool = http.get(format!("{main}/api/tags")).timeout(Duration::from_secs(2)).send().await.ok().map(|_| true).unwrap_or(false);
    if have {
        let v: Value = http.post(format!("{main}/api/chat")).timeout(Duration::from_secs(120)).json(&json!({"model": "qwen2.5-coder:1.5b", "stream": false, "options": {"temperature": 0.2, "num_predict": 600}, "messages": [{"role": "system", "content": sys}, {"role": "user", "content": user}]})).send().await?.json().await?;
        if let Some(t) = v["message"]["content"].as_str().filter(|t| !t.trim().is_empty()) {
            remember(user, t);
            return Ok(t.trim().to_string());
        }
    }
    chat(http, user).await
}

/// Perencana alat untuk model lokal: keluaran DIKUNCI ke skema JSON (alat harus dari daftar), plus contoh few-shot.
/// Teknik ini (constrained decoding + few-shot) membuat model 1B mampu memanggil alat secara andal.
/// Kalimat majemuk dipecah per klausa berurutan (model kecil lebih akurat per klausa), maks 4 langkah.
pub async fn plan(http: &reqwest::Client, goal: &str) -> Result<(Vec<crate::alat::Call>, String)> {
    let mut segs = vec![goal.to_string()];
    for sep in [" lalu ", " kemudian ", " terus ", " lantas ", " setelah itu ", " habis itu ", " selanjutnya ", ";"] {
        segs = segs.iter().flat_map(|s| s.split(sep).map(String::from).collect::<Vec<_>>()).collect();
    }
    segs.retain(|s| !s.trim().is_empty());
    if segs.len() < 2 {
        return plan_one(http, goal).await;
    }
    let (mut calls, mut reply) = (vec![], String::new());
    for s in segs.iter().take(4) {
        let (c, r) = plan_one(http, s.trim()).await?;
        calls.extend(c);
        if reply.is_empty() {
            reply = r;
        }
    }
    Ok((calls, reply))
}

async fn plan_one(http: &reqwest::Client, goal: &str) -> Result<(Vec<crate::alat::Call>, String)> {
    if !has(http, CHAT_MODEL).await {
        bail!("model obrolan lokal belum terpasang");
    }
    let tools = crate::gemini::tool_names();
    let schema = json!({
        "type": "object",
        "properties": {
            "calls": {"type": "array", "maxItems": 3, "items": {"type": "object", "properties": {"tool": {"type": "string", "enum": tools}, "arg": {"type": "string"}}, "required": ["tool", "arg"]}},
            "reply": {"type": "string"}
        },
        "required": ["calls", "reply"]
    });
    // sistem ringkas (prompt panjang membingungkan model kecil): daftar alat + aturan
    let sys = format!(
        "Kamu NOVA, asisten laptop yang ramah (panggil pengguna 'Bos'). Balas HANYA JSON {{\"calls\":[{{\"tool\":..,\"arg\":..}}],\"reply\":..}}.\nJika Bos meminta tindakan, isi calls dengan alat yang tepat dan reply satu kalimat ramah. Jika hanya ngobrol/bertanya umum, calls kosong dan reply jawaban singkat yang jujur.\nAlat:\n{}\nFakta: {}",
        crate::alat::TOOL_HELP.lines().chain(["atur <kecerahan|volume> <N>", "waktu", "web <pertanyaan>", "gambar <kata>", "jurnal <topik tahun>"]).map(|l| l.chars().take(110).collect::<String>()).collect::<Vec<_>>().join("\n"),
        crate::fakta::ringkas()
    );
    let shots: [(&str, &str); 11] = [
        ("cuaca di bandung", r#"{"calls":[{"tool":"cuaca","arg":"bandung"}],"reply":"Kucek cuaca Bandung ya Bos ☁"}"#),
        ("catat beli kabel besok", r#"{"calls":[{"tool":"catat","arg":"beli kabel besok"}],"reply":"Siap, kucatat Bos 📝"}"#),
        ("cek wifi di sini", r#"{"calls":[{"tool":"wifi","arg":""}],"reply":"Siap Bos, kucek Wi-Fi-nya ya 📶"}"#),
        ("buka kalkulator", r#"{"calls":[{"tool":"winapp","arg":"kalkulator"}],"reply":"Oke Bos, kubuka kalkulatornya 🧮"}"#),
        ("carikan file laporan pdf", r#"{"calls":[{"tool":"cari","arg":"laporan pdf"}],"reply":"Kucarikan di laptop ya Bos 🔎"}"#),
        ("berita listrik hari ini", r#"{"calls":[{"tool":"berita","arg":"listrik"}],"reply":"Kucek kabar terbarunya, Bos 📰"}"#),
        ("carikan jurnal deep learning 2021 sampai 2025", r#"{"calls":[{"tool":"jurnal","arg":"deep learning 2021 sampai 2025"}],"reply":"Kucari jurnalnya ya Bos 📚"}"#),
        ("kecerahan 40 persen", r#"{"calls":[{"tool":"atur","arg":"kecerahan 40"}],"reply":"Kuatur kecerahannya ya 🔆"}"#),
        ("buka youtube dan cari lagu santai", r#"{"calls":[{"tool":"open","arg":"https://www.youtube.com/results?search_query=lagu%20santai"}],"reply":"Kubuka pencarian YouTube-nya ya 🎵"}"#),
        ("rute ke stasiun bandung", r#"{"calls":[{"tool":"open","arg":"https://www.google.com/maps/search/stasiun%20bandung"}],"reply":"Kubuka petanya, Bos 🗺"}"#),
        ("halo apa kabar", r#"{"calls":[],"reply":"Halo Bos! Aku baik, siap bantu apa hari ini? 😊"}"#),
    ];
    let mut msgs = vec![json!({"role": "system", "content": sys})];
    for (u, a) in shots {
        msgs.push(json!({"role": "user", "content": u}));
        msgs.push(json!({"role": "assistant", "content": a}));
    }
    msgs.push(json!({"role": "user", "content": goal}));
    // Koreksi-diri: bila keluaran bukan JSON valid, kirim galatnya kembali ke model sekali lagi.
    let mut j: Option<Value> = None;
    for _ in 0..2 {
        let v: Value = http
            .post(format!("{URL}/api/chat"))
            .timeout(Duration::from_secs(90))
            .json(&json!({"model": CHAT_MODEL, "stream": false, "format": schema, "keep_alive": "3m", "options": {"temperature": 0.1, "num_predict": 200}, "messages": msgs}))
            .send()
            .await?
            .json()
            .await?;
        let raw = v["message"]["content"].as_str().unwrap_or("").to_string();
        match serde_json::from_str::<Value>(&raw) {
            Ok(x) => {
                j = Some(x);
                break;
            }
            Err(e) => {
                msgs.push(json!({"role": "assistant", "content": raw}));
                msgs.push(json!({"role": "user", "content": format!("Format JSON salah ({e}). Perbaiki: balas HANYA JSON {{\"calls\":[{{\"tool\":..,\"arg\":..}}],\"reply\":..}}.")}));
            }
        }
    }
    let j = j.ok_or_else(|| anyhow!("keluaran model bukan JSON"))?;
    let calls: Vec<crate::alat::Call> = j["calls"].as_array().map(|a| a.iter().filter_map(|c| Some(crate::alat::Call { tool: c["tool"].as_str()?.to_string(), arg: c["arg"].as_str().unwrap_or("").to_string() })).take(3).collect()).unwrap_or_default();
    // Penyaring: model kecil cenderung memanggil terlalu banyak alat / alat tanpa argumen wajib.
    const NEED_ARG: &[&str] = &["web", "cari", "gambar", "jurnal", "kode", "bukafile", "bukahasil", "fetch", "read", "ls", "write", "shell", "open", "winapp", "atur", "tanya", "remind", "tugas", "pelajari", "graphify", "kurikulum", "ringkas", "pdf", "bukapath", "cli"];
    let l = goal.to_lowercase();
    let multi = [" dan ", " lalu ", " kemudian ", " terus ", ";"].iter().any(|k| l.contains(k));
    let mut calls: Vec<crate::alat::Call> = calls
        .into_iter()
        .filter(|c| !(NEED_ARG.contains(&c.tool.as_str()) && c.arg.trim().is_empty()))
        .filter(|c| c.tool != "info" || l.contains("info"))
        .collect();
    if !multi {
        calls.truncate(1);
    }
    Ok((calls, j["reply"].as_str().unwrap_or("").to_string()))
}

pub fn looks_like_code(t: &str) -> bool {
    let l = t.to_lowercase();
    ["kode", "coding", "ngoding", "fungsi", "function", "bug", "error", "python", "rust", "javascript", "typescript", "script", "compile", "sql", "regex", "arduino", "ladder logic", "class ", "variabel", "algoritma", "```"].iter().any(|k| l.contains(k))
}

pub async fn chat(http: &reqwest::Client, user: &str) -> Result<String> {
    if !has(http, CHAT_MODEL).await {
        bail!("model obrolan lokal belum terpasang");
    }
    let sys = format!("Kamu NOVA, asisten pribadi yang hangat dan akrab di laptop. Panggil pengguna 'Bos'. Jawab dalam Bahasa Indonesia, singkat (maksimal 3 kalimat), ramah, dan jujur: bila tidak tahu atau butuh data terbaru, katakan terus terang. Jangan mengarang fakta. Jawab semua pertanyaan wajar tentang dirimu, komputer, kerja, dan kehidupan sehari-hari; JANGAN menolak atau berkata 'tidak dapat membantu' kecuali permintaannya benar-benar berbahaya.\nKonteks saat ini: {}. {}\nProfil Bos: {}\n{}", crate::sistem::waktu_text(), crate::hewan::memori(), crate::ingat::profile().into_iter().take(3).collect::<Vec<_>>().join(" "), crate::persona::ringkas(0));
    let mut msgs = vec![json!({"role": "system", "content": sys})];
    for (u, a) in HIST.lock().map(|h| h.clone()).unwrap_or_default() {
        msgs.push(json!({"role": "user", "content": u}));
        msgs.push(json!({"role": "assistant", "content": a}));
    }
    msgs.push(json!({"role": "user", "content": user}));
    let v: Value = http
        .post(format!("{URL}/api/chat"))
        .timeout(Duration::from_secs(90))
        .json(&json!({"model": CHAT_MODEL, "stream": false, "keep_alive": "3m", "options": {"temperature": 0.4, "num_predict": 260}, "messages": msgs}))
        .send()
        .await?
        .json()
        .await?;
    let t = v["message"]["content"].as_str().unwrap_or("").trim().to_string();
    if t.is_empty() {
        bail!("jawaban kosong");
    }
    remember(user, &t);
    crate::makan::feed(v["eval_count"].as_u64().unwrap_or(0) + v["prompt_eval_count"].as_u64().unwrap_or(0));
    Ok(t)
}

pub async fn status(http: &reqwest::Client) -> String {
    let m = models(http).await;
    if m.is_empty() {
        return format!("Model lokal: server {URL} belum hidup atau belum ada model (folder {MODELS_DIR}).");
    }
    format!("Model lokal aktif ({URL}): {}. Embedding {} · obrolan offline {}.", m.join(", "), if m.iter().any(|x| x.starts_with(EMBED_MODEL)) { "siap" } else { "belum" }, if m.iter().any(|x| x.starts_with("llama3.2")) { "siap" } else { "belum" })
}
