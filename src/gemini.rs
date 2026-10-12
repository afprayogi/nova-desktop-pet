//! Gemini Flash (tier gratis) sebagai "otak" cadangan untuk kalimat bebas: memetakan ucapan Bos ke alat NOVA (rencana JSON) atau
//! menjawab langsung, dan mencari informasi di internet (Google Search grounding). Urutan di NOVA: aturan/ML offline dulu (cepat,
//! gratis, privat) -> Gemini -> Llama lokal. API key disimpan terenkripsi DPAPI (`/gemini key <kunci>`). Yang dikirim ke Google
//! hanya teks perintah Bos + jam/mode kerja; isi file, layar (OCR), dan email TIDAK dikirim.
use crate::alat::Call;
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const DEFAULT_MODEL: &str = "gemini-2.5-flash";
static CALLS: Mutex<Vec<Instant>> = Mutex::new(Vec::new());
/// Model yang kuotanya habis / tidak boleh dipakai sementara: nama -> (sejak, lama).
static COOL: Mutex<Vec<(String, Instant, u64)>> = Mutex::new(Vec::new());

/// Status kesehatan Gemini: (offline sampai, jumlah gagal beruntun, pesan peristiwa untuk obrolan).
static HEALTH: Mutex<(Option<Instant>, u32, Option<String>)> = Mutex::new((None, 0, None));

/// Gemini bisa dipakai sekarang? (key ada, aktif, dan tidak sedang masa jeda setelah gagal).
pub fn gemini_ok() -> bool {
    ready() && HEALTH.lock().map(|h| h.0.map_or(true, |t| Instant::now() >= t)).unwrap_or(true)
}

/// Ada penyedia cloud yang bisa dipakai: Gemini sehat ATAU OmniRoute siap.
pub fn available() -> bool {
    gemini_ok() || crate::omniroute::usable()
}

/// Sedang dalam masa offline otomatis (bukan dimatikan manual)?
pub fn degraded() -> bool {
    ready() && !available()
}

fn mark_fail(err: &str) {
    if let Ok(mut h) = HEALTH.lock() {
        h.1 += 1;
        // jeda bertingkat: 2, 5, 15, 30 menit
        let mins = [2u64, 5, 15, 30][(h.1 as usize - 1).min(3)];
        h.0 = Some(Instant::now() + Duration::from_secs(mins * 60));
        if h.1 == 1 {
            h.2 = Some(format!("☁ Gemini bermasalah ({}). Beralih ke mode offline (Llama lokal); kucoba Gemini lagi {mins} menit lagi.", err.chars().take(80).collect::<String>()));
        }
    }
}

fn mark_ok() {
    if let Ok(mut h) = HEALTH.lock() {
        if h.1 > 0 {
            h.2 = Some("☁ Gemini pulih — kembali online. Model lokal dilepas dari RAM agar hemat.".into());
            crate::lokal::request_unload();
        }
        *h = (None, 0, h.2.take());
    }
}

/// Pesan peristiwa terbaru (beralih offline / pulih) untuk ditampilkan sekali.
pub fn take_event() -> Option<String> {
    HEALTH.lock().ok().and_then(|mut h| h.2.take())
}

fn cool(m: &str, secs: u64) {
    if let Ok(mut c) = COOL.lock() {
        c.retain(|x| x.0 != m);
        c.push((m.to_string(), Instant::now(), secs));
    }
}

fn cooled(m: &str) -> bool {
    COOL.lock().map(|c| c.iter().any(|x| x.0 == m && x.1.elapsed().as_secs() < x.2)).unwrap_or(false)
}

fn key_path() -> std::path::PathBuf {
    crate::config::data_dir().join("gemini.key")
}

fn off_path() -> std::path::PathBuf {
    crate::config::data_dir().join("gemini.off")
}

pub fn has_key() -> bool {
    key_path().exists()
}

/// Siap dipakai: ada key dan tidak dimatikan.
pub fn ready() -> bool {
    has_key() && !off_path().exists()
}

pub fn set_enabled(on: bool) {
    if on {
        let _ = std::fs::remove_file(off_path());
    } else {
        let _ = std::fs::write(off_path(), "1");
    }
}

pub fn set_key(k: &str) -> bool {
    let k = k.trim();
    if k.len() < 20 || k.contains(char::is_whitespace) {
        return false;
    }
    match crate::secure::protect(k) {
        Some(enc) => std::fs::write(key_path(), enc).is_ok(),
        None => false,
    }
}

pub(crate) fn api_key() -> Option<String> {
    key()
}

fn key() -> Option<String> {
    crate::secure::unprotect(std::fs::read_to_string(key_path()).ok()?.trim())
}

pub fn model() -> String {
    std::fs::read_to_string(crate::config::data_dir().join("gemini.model")).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| DEFAULT_MODEL.into())
}

pub fn set_model(m: &str) {
    let _ = std::fs::write(crate::config::data_dir().join("gemini.model"), m.trim());
}

/// Batas aman tier gratis: 12 permintaan per menit.
fn allow() -> bool {
    let Ok(mut c) = CALLS.lock() else { return false };
    c.retain(|t| t.elapsed() < Duration::from_secs(60));
    if c.len() >= 12 {
        return false;
    }
    c.push(Instant::now());
    true
}

fn text_of(v: &Value) -> String {
    v["candidates"][0]["content"]["parts"].as_array().map(|p| p.iter().filter_map(|x| x["text"].as_str()).collect::<Vec<_>>().join("")).unwrap_or_default()
}

/// Satu panggilan ke model tertentu.
async fn call(http: &reqwest::Client, k: &str, m: &str, system: &str, user: &str, json_out: bool, search: bool) -> Result<String> {
    // 400 "invalid argument" sering karena opsi yang tidak didukung model: coba ulang dengan opsi lebih sedikit.
    let mut last = String::new();
    for level in 0..3 {
        let mut gc = json!({"temperature": 0.3, "maxOutputTokens": 1200});
        if json_out && level < 2 {
            gc["responseMimeType"] = json!("application/json");
        }
        if level == 0 && (m.contains("2.5") || m.contains("latest")) {
            gc["thinkingConfig"] = json!({"thinkingBudget": 0});
        }
        let mut body = json!({
            "systemInstruction": {"parts": [{"text": system}]},
            "contents": [{"role": "user", "parts": [{"text": user}]}],
            "generationConfig": gc,
        });
        if search {
            body["tools"] = json!([{"google_search": {}}]);
        }
        let resp = http
            .post(format!("https://generativelanguage.googleapis.com/v1beta/models/{m}:generateContent"))
            .header("x-goog-api-key", k)
            .timeout(Duration::from_secs(40))
            .json(&body)
            .send()
            .await?;
        let status = resp.status();
        let v: Value = resp.json().await.unwrap_or(Value::Null);
        if !status.is_success() {
            let msg = v["error"]["message"].as_str().unwrap_or("").chars().take(200).collect::<String>();
            last = format!("HTTP {}: {msg}", status.as_u16());
            if status.as_u16() == 400 && !msg.to_lowercase().contains("api key") && level < 2 {
                continue;
            }
            bail!("{last}");
        }
        let t = text_of(&v);
        if t.trim().is_empty() {
            bail!("jawaban kosong");
        }
        crate::makan::feed(v["usageMetadata"]["totalTokenCount"].as_u64().unwrap_or(0));
        return Ok(t.trim().to_string());
    }
    bail!("{last}")
}

/// Model yang sudah tidak ada / tidak boleh dipakai akun ini (bukan sekadar gangguan sementara).
fn model_unusable(e: &str) -> bool {
    let l = e.to_lowercase();
    l.contains("http 404")
        || l.contains("no longer available")
        || l.contains("is not found")
        || l.contains("not supported")
        || l.contains("deprecated")
        || (l.contains("http 400") && l.contains("model"))
        || (l.contains("http 429") && (l.contains("limit: 0") || l.contains("free_tier") || l.contains("quota")))
}

/// Lama menghindari model setelah gagal: kuota harian -> 6 jam; kuota menit/lainnya -> 90 detik; model hilang -> 1 hari.
fn cool_secs(e: &str) -> u64 {
    let l = e.to_lowercase();
    if l.contains("http 404") || l.contains("no longer") || l.contains("deprecated") {
        86_400
    } else if l.contains("perday") || l.contains("per day") || l.contains("limit: 0") || l.contains("free_tier") {
        6 * 3600
    } else {
        90
    }
}

/// Pilih otomatis model flash gratis yang BENAR-BENAR bisa dipakai: coba kandidat berurutan dengan satu panggilan kecil.
pub async fn auto_setup(http: &reqwest::Client) -> Result<String> {
    let k = key().ok_or_else(|| anyhow!("API key Gemini belum diatur"))?;
    let ranked = rank_models(&list_models(http).await?);
    if ranked.is_empty() {
        bail!("tidak ada model flash di akun ini");
    }
    let mut last = String::new();
    for m in ranked.iter().take(6) {
        match call(http, &k, m, "Balas satu kata.", "Katakan: ok", false, false).await {
            Ok(_) => {
                set_model(m);
                return Ok(m.clone());
            }
            Err(e) => {
                cool(m, cool_secs(&e.to_string()));
                last = e.to_string();
            }
        }
    }
    bail!("tidak ada model yang bisa dipakai sekarang (terakhir: {last})")
}

/// Daftar model yang mendukung generateContent di akun ini.
pub async fn list_models(http: &reqwest::Client) -> Result<Vec<String>> {
    let k = key().ok_or_else(|| anyhow!("API key Gemini belum diatur"))?;
    let v: Value = http.get("https://generativelanguage.googleapis.com/v1beta/models?pageSize=200").header("x-goog-api-key", k).timeout(Duration::from_secs(15)).send().await?.json().await?;
    if let Some(m) = v["error"]["message"].as_str() {
        bail!("{m}");
    }
    Ok(v["models"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|m| m["supportedGenerationMethods"].as_array().is_some_and(|x| x.iter().any(|y| y == "generateContent")))
                .filter_map(|m| m["name"].as_str().map(|n| n.trim_start_matches("models/").to_string()))
                .collect()
        })
        .unwrap_or_default())
}

/// Urutan kandidat model teks "flash": alias -latest, stabil terbaru, lalu lite, lalu preview.
pub fn rank_models(all: &[String]) -> Vec<String> {
    let bad = ["image", "tts", "audio", "live", "embedding", "aqa", "vision", "robotics", "computer", "native", "thinking", "learnlm", "gemma"];
    let mut v: Vec<(f32, String)> = all
        .iter()
        .filter(|n| n.starts_with("gemini") && n.contains("flash") && !bad.iter().any(|b| n.contains(b)))
        .map(|n| {
            let ver: f32 = n.split('-').find_map(|p| p.parse::<f32>().ok()).unwrap_or(0.0);
            let mut sc = ver * 10.0;
            if n.contains("latest") {
                sc += 200.0;
            }
            if !n.contains("preview") && !n.contains("exp") {
                sc += 100.0;
            }
            if n.contains("lite") {
                sc -= 25.0;
            }
            (sc, n.clone())
        })
        .collect();
    v.sort_by(|a, b| b.0.total_cmp(&a.0));
    v.into_iter().map(|x| x.1).collect()
}

pub async fn generate(http: &reqwest::Client, system: &str, user: &str, json_out: bool, search: bool) -> Result<String> {
    // Rantai penyedia: Gemini (bila sehat) → OmniRoute → (pemanggil beralih ke Llama lokal)
    if !search && !gemini_ok() && crate::omniroute::usable() {
        return crate::omniroute::chat(http, system, user, json_out).await;
    }
    let r = generate_inner(http, system, user, json_out, search).await;
    if r.is_err() && !search && crate::omniroute::usable() {
        if let Ok(t) = crate::omniroute::chat(http, system, user, json_out).await {
            if let Err(e) = &r {
                mark_fail(&e.to_string());
            }
            return Ok(t);
        }
    }
    match &r {
        Ok(_) => mark_ok(),
        // pencarian Google punya kuota terpisah: kegagalannya bukan tanda Gemini mati
        Err(e) if !search => mark_fail(&e.to_string()),
        Err(_) => {}
    }
    r
}

async fn generate_inner(http: &reqwest::Client, system: &str, user: &str, json_out: bool, search: bool) -> Result<String> {
    let k = key().ok_or_else(|| anyhow!("API key Gemini belum diatur (/gemini key <kunci>)"))?;
    if !allow() {
        bail!("batas 12 permintaan/menit tier gratis tercapai, coba sebentar lagi");
    }
    let mut m = model();
    let mut tried: Vec<String> = vec![];
    let mut cands: Vec<String> = vec![];
    for attempt in 0..6 {
        if cooled(&m) {
            // model pilihan sedang dihindari: pindah otomatis ke kandidat lain yang belum dicoba
            tried.push(m.clone());
            if cands.is_empty() {
                cands = rank_models(&list_models(http).await.unwrap_or_default());
            }
            match cands.iter().find(|c| !tried.contains(c) && !cooled(c)) {
                Some(n) => m = n.clone(),
                None => bail!("semua model Gemini yang tersedia sedang kehabisan kuota; coba lagi nanti"),
            }
        }
        match call(http, &k, &m, system, user, json_out, search).await {
            Ok(t) => {
                if attempt > 0 {
                    set_model(&m); // model yang berhasil menjadi pilihan baru
                }
                return Ok(t);
            }
            // kuota pencarian (grounding) terpisah dari kuota model: jangan memblacklist model karenanya
            Err(e) if attempt < 5 && !search && model_unusable(&e.to_string()) => {
                cool(&m, cool_secs(&e.to_string()));
                tried.push(m.clone());
                if cands.is_empty() {
                    cands = rank_models(&list_models(http).await.unwrap_or_default());
                }
                match cands.iter().find(|c| !tried.contains(c) && !cooled(c)) {
                    Some(n) => m = n.clone(),
                    None => return Err(anyhow!("model '{}' tidak bisa dipakai dan tidak ada model flash lain di akun ini ({e})", tried.join(", "))),
                }
            }
            Err(e) => return Err(e),
        }
    }
    bail!("semua model Gemini yang dicoba gagal")
}

pub struct Plan {
    pub calls: Vec<Call>,
    pub reply: String,
    pub cached: bool,
}

// ---------------- memori hasil Gemini (hemat token) ----------------

fn cdb() -> Option<rusqlite::Connection> {
    let c = crate::assistant::db().ok()?;
    c.execute_batch("CREATE TABLE IF NOT EXISTS gcache(k TEXT, kind TEXT, val TEXT, ts INT, hits INT DEFAULT 0, PRIMARY KEY(k, kind));").ok()?;
    Some(c)
}

fn norm(q: &str) -> Vec<String> {
    let mut w: Vec<String> = q.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(String::from).collect();
    w.sort();
    w.dedup();
    w
}

/// Cari jawaban/rencana yang pernah disimpan: kunci identik, atau kalimat sangat mirip (Jaccard >= 0,8, minimal 3 kata).
fn cache_get(kind: &str, q: &str, ttl_s: i64) -> Option<String> {
    let w = norm(q);
    if w.len() < 3 {
        return None;
    }
    let c = cdb()?;
    let k = w.join(" ");
    let min_ts = chrono::Local::now().timestamp() - ttl_s;
    let hit = |k2: &str, val: String| {
        let _ = c.execute("UPDATE gcache SET hits=hits+1 WHERE k=?1 AND kind=?2", rusqlite::params![k2, kind]);
        Some(val)
    };
    if let Ok(v) = c.query_row("SELECT val FROM gcache WHERE k=?1 AND kind=?2 AND ts>=?3", rusqlite::params![k, kind, min_ts], |r| r.get::<_, String>(0)) {
        return hit(&k, v);
    }
    let mut st = c.prepare("SELECT k, val FROM gcache WHERE kind=?1 AND ts>=?2 ORDER BY ts DESC LIMIT 400").ok()?;
    let rows: Vec<(String, String)> = st.query_map(rusqlite::params![kind, min_ts], |r| Ok((r.get(0)?, r.get(1)?))).ok()?.flatten().collect();
    for (k2, v) in rows {
        let w2: Vec<&str> = k2.split(' ').collect();
        let inter = w.iter().filter(|x| w2.contains(&x.as_str())).count() as f32;
        let uni = (w.len() + w2.len()) as f32 - inter;
        if uni > 0.0 && inter / uni >= 0.8 {
            return hit(&k2, v);
        }
    }
    None
}

fn cache_put(kind: &str, q: &str, val: &str) {
    let w = norm(q);
    if w.len() < 3 || val.trim().is_empty() {
        return;
    }
    if let Some(c) = cdb() {
        let _ = c.execute("INSERT OR REPLACE INTO gcache(k, kind, val, ts, hits) VALUES(?1, ?2, ?3, ?4, COALESCE((SELECT hits FROM gcache WHERE k=?1 AND kind=?2), 0))", rusqlite::params![w.join(" "), kind, val, chrono::Local::now().timestamp()]);
        let _ = c.execute("DELETE FROM gcache WHERE ts < ?1", [chrono::Local::now().timestamp() - 90 * 86400]);
    }
}

pub fn cache_stats() -> (i64, i64) {
    cdb().and_then(|c| c.query_row("SELECT COUNT(*), COALESCE(SUM(hits),0) FROM gcache", [], |r| Ok((r.get(0)?, r.get(1)?))).ok()).unwrap_or((0, 0))
}

pub fn cache_clear() -> usize {
    cdb().and_then(|c| c.execute("DELETE FROM gcache", []).ok()).unwrap_or(0)
}

// ---------------- konteks percakapan ----------------

static TURNS: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

/// Catat satu giliran (ucapan Bos, ringkasan hasil) agar Gemini memahami kelanjutan ("yang tadi", "buka yang kedua").
pub fn remember_turn(user: &str, outcome: &str) {
    if let Ok(mut t) = TURNS.lock() {
        t.push((user.chars().take(160).collect(), outcome.chars().take(160).collect()));
        if t.len() > 4 {
            t.remove(0);
        }
    }
}

fn recent_turns() -> String {
    TURNS.lock().map(|t| t.iter().map(|(u, o)| format!("Bos: {u}\nNOVA: {o}")).collect::<Vec<_>>().join("\n")).unwrap_or_default()
}

pub(crate) const EXTRA_TOOLS: &str = "\
atur <kecerahan|volume> <N|+N|-N|mute|unmute|?> : atur kecerahan layar / volume suara (persen)
waktu         : jam, hari, dan tanggal sekarang
web <pertanyaan>: cari informasi terbaru di internet lalu rangkum
wifi / port / bukafile <nama> / cli <alat>: lihat Wi-Fi, port aktif, buka berkas langsung, bantuan CLI
winapp <nama> : buka aplikasi bawaan Windows (kalkulator, notepad, paint, snipping tool, task manager, pengaturan wifi/bluetooth/suara/tampilan, windows update, control panel, alarm, kalender, foto, cuaca, device manager)";

/// Petakan ucapan Bos ke rencana alat NOVA (maks 4) atau jawaban langsung.
pub async fn plan(http: &reqwest::Client, goal: &str) -> Result<Plan> {
    // Kalimat yang bergantung pada percakapan sebelumnya tidak dicache.
    let ctx = recent_turns();
    let cacheable = ctx.is_empty() || !["tadi", "itu", "tersebut", "yang kedua", "yang pertama", "lanjut", "lagi", "sama"].iter().any(|k| goal.to_lowercase().contains(k));
    if cacheable {
        if let Some(j) = cache_get("plan", goal, 30 * 86400) {
            if let Some(mut p) = parse_plan(&j) {
                p.cached = true;
                return Ok(p);
            }
        }
    }
    let sys = plan_sys(&ctx);
    let raw = generate(http, &sys, goal, true, false).await?;
    let raw = raw.trim().trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```").trim();
    let p = parse_plan(raw).ok_or_else(|| anyhow!("format jawaban tidak dikenali"))?;
    if cacheable {
        cache_put("plan", goal, raw);
    }
    remember_turn(goal, &if p.calls.is_empty() { p.reply.clone() } else { format!("menjalankan {}", p.calls.iter().map(|c| c.tool.as_str()).collect::<Vec<_>>().join(", ")) });
    Ok(p)
}

/// Prompt sistem perencana alat (dipakai Gemini dan Llama lokal).
pub fn plan_sys(ctx: &str) -> String {
    let (mode, _) = crate::konteks::current();
    format!(
        "Kamu NOVA, asisten pribadi di laptop Windows milik Bos (insinyur listrik). Sekarang: {}. Mode kerja: {:?}.\n\
{}\n\
Tugasmu: pahami ucapan Bos lalu balas HANYA JSON: {{\"calls\":[{{\"tool\":\"nama\",\"arg\":\"...\"}}],\"reply\":\"...\"}}.\n\
- Bila ucapan meminta tindakan yang ada di daftar alat, isi calls (maksimal 4, urut) dan isi reply dengan SATU kalimat ramah pengantar (mis. \"Siap Bos, kucek Wi-Fi-nya dulu ya 📶\").\n\
- Bila hanya bertanya/ngobrol, calls kosong dan reply berisi jawaban ramah (maksimal 4 kalimat, Bahasa Indonesia, jangan mengarang).\n\
- Pertanyaan tentang fakta terbaru/berita/harga/cuaca -> pakai alat web. Jangan memakai alat yang tidak ada di daftar.\n\
- shell/write/open hanya bila Bos jelas memintanya; keputusan akhir tetap di Bos (akan dimintai konfirmasi).\n\
Alat (nama argumen: deskripsi [risiko]):\n{}\n\nKonteks: aplikasi di depan = {}. {}{}",
        crate::sistem::waktu_text(),
        mode,
        crate::persona::prompt(0),
        crate::registry::teks(None),
        crate::konteks::current_app(),
        crate::hewan::memori(),
        if ctx.is_empty() { String::new() } else { format!("\nPercakapan terakhir (untuk memahami rujukan):\n{ctx}") }
    )
}

/// Nama semua alat yang boleh dipanggil (untuk membatasi keluaran terstruktur model lokal).
pub fn tool_names() -> Vec<String> {
    let mut v: Vec<String> = crate::alat::TOOL_HELP
        .lines()
        .chain(EXTRA_TOOLS.lines())
        .filter_map(|l| l.split(|c: char| c.is_whitespace() || c == ':').next())
        .filter(|w| !w.is_empty() && w.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'))
        .map(String::from)
        .collect();
    v.sort();
    v.dedup();
    v
}

fn parse_plan(raw: &str) -> Option<Plan> {
    let v: Value = serde_json::from_str(raw).ok().or_else(|| crate::alat::parse_json(raw))?;
    let calls = v["calls"]
        .as_array()
        .map(|a| a.iter().filter_map(|c| Some(Call { tool: c["tool"].as_str()?.trim().to_lowercase(), arg: c["arg"].as_str().unwrap_or("").to_string() })).take(4).collect())
        .unwrap_or_default();
    Some(Plan { calls, reply: v["reply"].as_str().unwrap_or("").to_string(), cached: false })
}

/// Cari di internet. Dengan key: Gemini + Google Search. Tanpa key: Wikipedia (id) + DuckDuckGo instant answer.
pub async fn web(http: &reqwest::Client, q: &str) -> String {
    let mut gerr: Option<String> = None;
    if ready() {
        if let Some(t) = cache_get("web", q, 6 * 3600) {
            return format!("💾 {t}");
        }
        let sys = format!("Kamu NOVA, asisten yang hangat dan akrab (panggil \"Bos\"). Jawab dalam Bahasa Indonesia, ringkas (maksimal 5 kalimat), ramah, berdasarkan hasil pencarian terbaru. Sekarang: {}.", crate::sistem::waktu_text());
        match generate(http, &sys, q, false, true).await {
            Ok(t) => {
                cache_put("web", q, &t);
                return format!("🌐 {t}");
            }
            Err(e) => {
                gerr = Some(e.to_string());
                // Kuota pencarian Google (grounding) di tier gratis terbatas: jawab dari pengetahuan model, beri tahu Bos.
                let sys2 = "Kamu NOVA, asisten yang hangat dan akrab (panggil \"Bos\"). Jawab dalam Bahasa Indonesia, ringkas (maksimal 4 kalimat), ramah. Kamu TIDAK punya akses internet saat ini: jawab dari pengetahuanmu dan katakan jelas bila datanya bisa sudah berubah (harga, berita, cuaca).";
                if let Ok(t) = generate(http, sys2, q, false, false).await {
                    return format!("🌐 (pencarian langsung tidak tersedia: kuota Google Search habis; ini dari pengetahuan Gemini, bisa tidak terbaru)
{t}");
                }
            }
        }
    }
    let enc = urlencoding::encode(q).to_string();
    if let Ok(r) = http.get(format!("https://id.wikipedia.org/api/rest_v1/page/summary/{}", urlencoding::encode(&q.replace(' ', "_")))).timeout(Duration::from_secs(10)).send().await {
        if let Ok(v) = r.json::<Value>().await {
            if let Some(t) = v["extract"].as_str().filter(|t| !t.is_empty()) {
                return format!("🌐 Wikipedia: {}", crate::research::truncate(t, 700));
            }
        }
    }
    if let Ok(r) = http.get(format!("https://api.duckduckgo.com/?q={enc}&format=json&no_html=1&skip_disambig=1")).timeout(Duration::from_secs(10)).send().await {
        if let Ok(v) = r.json::<Value>().await {
            if let Some(t) = v["AbstractText"].as_str().filter(|t| !t.is_empty()) {
                return format!("🌐 DuckDuckGo: {}", crate::research::truncate(t, 700));
            }
        }
    }
    let why = match gerr {
        Some(e) => format!("Gemini gagal mencari ({e})."),
        None => "Pasang key Gemini (/gemini key <kunci>) agar bisa mencari di Google.".into(),
    };
    format!("🌐 Tidak ada ringkasan instan untuk '{q}'. {why} Pencarian manual: https://duckduckgo.com/?q={enc}")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cache_reuses_similar_questions_and_expires() {
        let q = "zzuji berapa tegangan standar pln rumah tangga";
        cache_put("uji", q, "220 V");
        assert_eq!(cache_get("uji", q, 3600).as_deref(), Some("220 V"));
        assert_eq!(cache_get("uji", "zzuji tegangan standar pln rumah tangga berapa", 3600).as_deref(), Some("220 V"));
        assert!(cache_get("uji", "tentang kucing dan anjing peliharaan", 3600).is_none());
        assert!(cache_get("uji", q, -10).is_none());
        if let Some(c) = cdb() {
            let _ = c.execute("DELETE FROM gcache WHERE kind='uji'", []);
        }
    }

    #[test]
    fn failover_state_machine() {
        if let Ok(mut h) = HEALTH.lock() {
            *h = (None, 0, None);
        }
        mark_fail("HTTP 429 uji");
        assert!(HEALTH.lock().unwrap_or_else(|e| e.into_inner()).0.is_some());
        assert!(take_event().unwrap().contains("offline"));
        mark_ok();
        assert!(HEALTH.lock().unwrap_or_else(|e| e.into_inner()).0.is_none());
        assert!(take_event().unwrap().contains("pulih"));
    }

    #[test]
    fn rate_limit_and_key_validation() {
        assert!(!set_key("pendek"));
        assert!(!set_key("ada spasi di dalam key yang panjang sekali"));
        let mut ok = 0;
        for _ in 0..20 {
            if allow() {
                ok += 1;
            }
        }
        assert!(ok <= 12);
    }
}
