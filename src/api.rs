//! API lokal TANPA tampilan (hanya 127.0.0.1): peliharaan membaca keadaan dan mengirim perintah.
//! Aman dari situs web: butuh header x-nova-token (dibaca dari file di folder data pengguna) dan JSON.
use crate::assistant::AssistState;
use crate::config::{self, Config};
use crate::engine::{self, Approval, Cmd, Event, Tx, Who};
use crate::monitor::Health;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    response::{Html, IntoResponse, Response},
    Json, Router,
};
use rand::Rng;
use serde::Serialize;
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

/// Dinaikkan oleh menu tray agar peliharaan membuka bilah obrolan.
pub static OPEN_CHAT: AtomicU64 = AtomicU64::new(0);
/// Naik tiap kali Bos mengetik "obrolan": peliharaan membuka layar obrolan besar.
pub static OPEN_BIG: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Serialize)]
struct ChatDto {
    seq: u64,
    /// -1 Bos, -2 sistem, >=0 indeks agen
    who: i32,
    text: String,
}

#[derive(Default)]
struct Snap {
    chat: VecDeque<ChatDto>,
    seq: u64,
    approval: Option<Approval>,
    health: Option<Health>,
    assist: Option<AssistState>,
    scene_id: u64,
    scene_lines: Vec<(usize, String)>,
    scene_started: Option<Instant>,
    notices: VecDeque<String>,
    notice_seq: u64,
    term_seq: u64,
    term: String,
    thinking: bool,
    emo: String,
    emo_at: Option<Instant>,
    emo_level: f32,
    feedback_until: Option<Instant>,
    ask: Option<(String, Vec<String>)>,
    act: Option<(String, String, Instant, u32)>,
}

impl Snap {
    fn add_notice(&mut self, t: String) {
        self.notice_seq += 1;
        self.notices.push_back(format!("{} {}", chrono::Local::now().format("%H:%M"), t.chars().take(400).collect::<String>()));
        while self.notices.len() > 30 {
            self.notices.pop_front();
        }
    }

    fn apply(&mut self, e: &Event) {
        match e {
            Event::Chat(l) => {
                if matches!(l.who, Who::Boss) {
                    self.ask = None;
                }
                self.seq += 1;
                let who = match l.who {
                    Who::Boss => -1,
                    Who::System => -2,
                    Who::Agent(i) => i as i32,
                };
                self.chat.push_back(ChatDto { seq: self.seq, who, text: l.text.clone() });
                while self.chat.len() > 60 {
                    self.chat.pop_front();
                }
            }
            Event::ChatReset => self.chat.clear(),
            Event::Approval(a) => self.approval = a.clone(),
            Event::Health(h) => self.health = Some((**h).clone()),
            Event::Assist(a) => self.assist = Some((**a).clone()),
            Event::Notice(t) => self.add_notice(t.clone()),
            Event::Term { cmd, out } => {
                self.term_seq += 1;
                self.term = format!("$ {cmd}\n{out}");
            }
            Event::Scene(l) => {
                for (i, t) in l {
                    self.add_notice(format!("{}: {t}", crate::agents::PERSONAS[*i % crate::agents::N].name));
                }
                self.scene_id += 1;
                self.scene_lines = l.clone();
                self.scene_started = Some(Instant::now());
            }
            Event::Thinking(t) => self.thinking = t.is_some(),
            Event::Ask { question, options } => self.ask = Some((question.clone(), options.clone())),
            Event::Act { kind, text, ms } => self.act = Some((kind.clone(), text.clone(), Instant::now(), *ms)),
            Event::Mood { emo, level } => {
                self.emo = emo.clone();
                self.emo_level = *level;
                self.emo_at = Some(Instant::now());
            }
            Event::FeedbackAsk(b) => self.feedback_until = b.then(|| Instant::now() + std::time::Duration::from_secs(300)),
            _ => {}
        }
    }
}

struct Shared {
    cmd: UnboundedSender<Cmd>,
    snap: Mutex<Snap>,
    token: String,
}

fn authorized(h: &HeaderMap, s: &Shared) -> bool {
    h.get("x-nova-token").and_then(|v| v.to_str().ok()) == Some(s.token.as_str())
}

#[derive(Serialize)]
struct PetDto {
    name: &'static str,
    mood: &'static str,
    status: String,
    next: String,
    /// Agen yang paling cocok dengan konteks (0 NOVA, 1 VOLT, 2 SENTRY, 3 MEMO).
    active: usize,
    mode: String,
    user_idle: u64,
    scene_id: u64,
    /// Detik sejak adegan mulai (-1 = tidak ada).
    scene_age: f32,
    lines: Vec<(usize, String)>,
    chat: Vec<ChatDto>,
    approval: Option<Approval>,
    thinking: bool,
    open_chat: u64,
    open_big: u64,
    emo: String,
    emo_level: f32,
    /// Detik sejak emosi terdeteksi (-1 = tidak ada).
    emo_age: f32,
    feedback: bool,
    macros: Vec<String>,
    bec_due: bool,
    ask: Option<AskDto>,
    act: String,
    act_text: String,
    karakter: String,
    topi: String,
    karakter_daftar: Vec<String>,
    notices: Vec<String>,
    notice_seq: u64,
    term_seq: u64,
    term: String,
    /// Detik sejak antarmuka web terakhir menyapa server (-1 = tidak terbuka).
    ui_age: f32,
    typing: bool,
    media: String,
    /// Detik sejak aksi mulai (-1 = tidak ada); act_ms = durasi.
    act_age: f32,
    act_ms: u32,
}

#[derive(Serialize)]
struct AskDto {
    question: String,
    options: Vec<String>,
}

fn macros_cached() -> Vec<String> {
    static C: Mutex<Option<(Instant, Vec<String>)>> = Mutex::new(None);
    let mut g = C.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((t, v)) = g.as_ref() {
        if t.elapsed().as_secs() < 10 {
            return v.clone();
        }
    }
    let v = crate::makro::names();
    *g = Some((Instant::now(), v.clone()));
    v
}

static LAST_UI: Mutex<Option<Instant>> = Mutex::new(None);

fn ui_age() -> f32 {
    LAST_UI.lock().ok().and_then(|g| g.map(|t| t.elapsed().as_secs_f32())).filter(|a| *a < 60.0).unwrap_or(-1.0)
}

async fn pet(State(s): State<Arc<Shared>>, headers: HeaderMap) -> Result<Json<PetDto>, StatusCode> {
    if !authorized(&headers, &s) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    if headers.contains_key("x-nova-ui") {
        if let Ok(mut g) = LAST_UI.lock() {
            *g = Some(Instant::now());
        }
    }
    let g = s.snap.lock().unwrap_or_else(|e| e.into_inner());
    let (bed, wake) = crate::assistant::sleep_window();
    let m = crate::assistant::day_min();
    let night = if bed > wake { m >= bed || m < wake } else { m >= bed && m < wake };
    let (mut mood, mut status) = ("ok", String::new());
    if let Some(h) = &g.health {
        let ram = h.ram_used_mb * 100 / h.ram_total_mb.max(1);
        status = format!("CPU {:.0}% · RAM {}% · Disk {}%", h.cpu, ram, h.disk_used_gb * 100 / h.disk_total_gb.max(1));
        if h.cpu > 85.0 {
            mood = "panas";
        } else if ram >= 88 {
            mood = "waspada";
        }
    }
    if night && mood == "ok" {
        mood = "tidur";
    }
    let (cmode, user_idle) = crate::konteks::current();
    Ok(Json(PetDto {
        name: crate::agents::NAME,
        mood,
        status,
        next: g.assist.as_ref().and_then(|a| a.agenda.first().cloned()).unwrap_or_default(),
        active: if crate::konteks::OVERRIDE.load(std::sync::atomic::Ordering::Relaxed) < 4 { cmode.agent() } else if mood == "panas" || mood == "waspada" { 2 } else { cmode.agent() },
        mode: cmode.label().to_string(),
        user_idle,
        scene_id: g.scene_id,
        scene_age: g.scene_started.map(|t| t.elapsed().as_secs_f32()).unwrap_or(-1.0),
        lines: g.scene_lines.clone(),
        chat: g.chat.iter().rev().take(30).rev().cloned().collect(),
        approval: g.approval.clone(),
        thinking: g.thinking,
        open_chat: OPEN_CHAT.load(Ordering::Relaxed),
        open_big: OPEN_BIG.load(Ordering::Relaxed),
        emo: g.emo.clone(),
        emo_level: g.emo_level,
        emo_age: g.emo_at.map(|t| t.elapsed().as_secs_f32()).unwrap_or(-1.0),
        feedback: g.feedback_until.is_some_and(|t| Instant::now() < t),
        macros: macros_cached(),
        bec_due: crate::bec::due(),
        ask: g.ask.clone().map(|(question, options)| AskDto { question, options }),
        act: g.act.as_ref().map(|a| a.0.clone()).unwrap_or_default(),
        act_text: g.act.as_ref().map(|a| a.1.clone()).unwrap_or_default(),
        notices: g.notices.iter().rev().take(15).cloned().collect(),
        notice_seq: g.notice_seq,
        karakter: crate::karakter::aktif(),
        topi: crate::widget::topi(),
        karakter_daftar: crate::karakter::daftar(),
        term_seq: g.term_seq,
        term: g.term.clone(),
        ui_age: ui_age(),
        typing: crate::ambien::typing(),
        media: crate::ambien::media().to_string(),
        act_age: g.act.as_ref().map(|a| a.2.elapsed().as_secs_f32()).unwrap_or(-1.0),
        act_ms: g.act.as_ref().map(|a| a.3).unwrap_or(0),
    }))
}

/// Soal tes BEC hari ini (hanya bila memang waktunya).
async fn bec_test(State(s): State<Arc<Shared>>, headers: HeaderMap) -> Result<Json<crate::bec::TestSet>, StatusCode> {
    if !authorized(&headers, &s) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    if crate::bec::due() { Ok(Json(crate::bec::build_test())) } else { Err(StatusCode::NOT_FOUND) }
}

#[derive(serde::Deserialize)]
struct AdReport {
    host: String,
}

/// Laporan iklan dari ekstensi browser.
async fn post_ad(State(s): State<Arc<Shared>>, headers: HeaderMap, Json(r): Json<AdReport>) -> StatusCode {
    if !authorized(&headers, &s) {
        return StatusCode::UNAUTHORIZED;
    }
    crate::adblock::report_external(&r.host.chars().take(120).collect::<String>());
    StatusCode::OK
}

async fn post_cmd(State(s): State<Arc<Shared>>, headers: HeaderMap, Json(c): Json<Cmd>) -> StatusCode {
    if !authorized(&headers, &s) {
        return StatusCode::UNAUTHORIZED;
    }
    let _ = s.cmd.send(c);
    StatusCode::OK
}

const UI_HTML: &str = include_str!("ui.html");
const DASH_HTML: &str = include_str!("dash.html");

fn local_host(h: &HeaderMap) -> bool {
    let host = h.get("host").and_then(|h| h.to_str().ok()).unwrap_or("");
    host.starts_with("127.0.0.1:") || host.starts_with("localhost:")
}

async fn dash(State(s): State<Arc<Shared>>, headers: HeaderMap) -> Response {
    if !local_host(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    Html(DASH_HTML.replace("__TOKEN__", &s.token)).into_response()
}

/// Data dasbor: tentang Bos (pola, kelelahan, aplikasi), laptop (CPU/RAM/disk/proses), dan otak NOVA (ML, contoh, cache).
#[derive(serde::Deserialize)]
struct OmniReq {
    url: String,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    hapus: bool,
}

/// Bedah satu kalimat TANPA menjalankannya: tiap tahap pemahaman NOVA dan siapa yang akhirnya memutuskan. `?llm=1` menambah rencana Llama lokal.
async fn bedah_get(State(s): State<Arc<Shared>>, headers: HeaderMap, axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>) -> Result<Json<serde_json::Value>, StatusCode> {
    if !authorized(&headers, &s) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let t = q.get("q").cloned().unwrap_or_default();
    if t.trim().is_empty() {
        return Ok(Json(serde_json::json!({"galat": "kalimat kosong"})));
    }
    use serde_json::json;
    let norm = crate::rawat::normalisasi(&t);
    let cj = |c: &crate::alat::Call| {
        let r = match crate::alat::assess(c) {
            crate::alat::Risk::Safe => "aman",
            crate::alat::Risk::Confirm(_) => "minta persetujuan",
            crate::alat::Risk::Block(_) => "diblokir",
        };
        json!({"alat": c.tool, "arg": c.arg, "risiko": r})
    };
    let ajar = crate::ajar::cocok(&norm).map(|c| cj(&c));
    let dasar = crate::aturan::rule_dasar(&norm).map(|c| cj(&c));
    let http = crate::research::client(20);
    let sem = crate::semantik::scores(&http, &norm).await.ok().map(|v| v.into_iter().take(3).map(|(l, s)| json!([l, (s * 100.0).round() / 100.0])).collect::<Vec<_>>());
    let sem_jelas = crate::semantik::classify(&http, &norm).await.map(|(l, s)| json!([l, (s * 100.0).round() / 100.0]));
    let ml = crate::ml::classify(&norm).map(|(l, s)| json!([l, s]));
    let llm = if q.contains_key("llm") {
        match crate::lokal::plan(&http, &norm).await {
            Ok((c, r)) => json!({"calls": c.iter().map(&cj).collect::<Vec<_>>(), "balasan": r}),
            Err(e) => json!({"galat": e.to_string()}),
        }
    } else {
        serde_json::Value::Null
    };
    let cad = crate::aturan::layanan_cadangan(&norm).map(|c| cj(&c));
    let keputusan = if ajar.is_some() {
        "aturan yang kamu ajarkan"
    } else if dasar.is_some() {
        "aturan bahasa bawaan"
    } else if sem_jelas.is_some() {
        "pemahaman makna (embedding)"
    } else if ml.is_some() {
        "tebakan ML dari contoh"
    } else if cad.is_some() {
        "layanan web cadangan (layanan.json, pemicu umum)"
    } else {
        "tidak dikenali lokal: diserahkan ke model bahasa (Gemini, lalu Llama)"
    };
    Ok(Json(json!({"norm": norm, "ajar": ajar, "aturan": dasar, "semantik": sem, "semantik_jelas": sem_jelas, "ml": ml, "cadangan": cad, "llm": llm, "keputusan": keputusan})))
}

async fn ajar_get(State(s): State<Arc<Shared>>, headers: HeaderMap) -> Result<Json<serde_json::Value>, StatusCode> {
    if !authorized(&headers, &s) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(Json(serde_json::json!({"aturan": crate::ajar::daftar(), "alat": crate::gemini::tool_names()})))
}

/// Ajarkan aturan baru `{pola, alat, arg}` atau hapus `{hapus: indeks}`.
async fn ajar_post(State(s): State<Arc<Shared>>, headers: HeaderMap, Json(r): Json<serde_json::Value>) -> Result<Json<serde_json::Value>, StatusCode> {
    if !authorized(&headers, &s) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    if let Some(i) = r["hapus"].as_u64() {
        return Ok(Json(serde_json::json!({"ok": crate::ajar::hapus(i as usize)})));
    }
    let g = |k: &str| r[k].as_str().unwrap_or("").to_string();
    Ok(Json(match crate::ajar::tambah(&g("pola"), &g("alat"), &g("arg")) {
        Ok(m) => serde_json::json!({"ok": true, "pesan": m}),
        Err(e) => serde_json::json!({"ok": false, "pesan": e}),
    }))
}

/// Daftar alat berskema (bentuk MCP `tools/list`).
async fn putusan_get(State(s): State<Arc<Shared>>, headers: HeaderMap) -> Result<Json<serde_json::Value>, StatusCode> {
    if !authorized(&headers, &s) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(Json(serde_json::to_value(crate::putusan::daftar()).unwrap_or_default()))
}

/// `{i, aksi: setuju|tolak|hapus}` atas putusan tersimpan.
async fn putusan_post(State(s): State<Arc<Shared>>, headers: HeaderMap, Json(r): Json<serde_json::Value>) -> Result<Json<serde_json::Value>, StatusCode> {
    if !authorized(&headers, &s) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let ok = crate::putusan::aksi(r["i"].as_u64().unwrap_or(u64::MAX) as usize, r["aksi"].as_str().unwrap_or(""));
    Ok(Json(serde_json::json!({"ok": ok})))
}

async fn alat_get(State(s): State<Arc<Shared>>, headers: HeaderMap) -> Result<Json<serde_json::Value>, StatusCode> {
    if !authorized(&headers, &s) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(Json(crate::registry::mcp()))
}

async fn graf_get(State(s): State<Arc<Shared>>, headers: HeaderMap) -> Result<Json<serde_json::Value>, StatusCode> {
    if !authorized(&headers, &s) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(Json(tokio::task::spawn_blocking(|| crate::graf::json(350)).await.unwrap_or_default()))
}

async fn otak_get(State(s): State<Arc<Shared>>, headers: HeaderMap, axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>) -> Result<Json<serde_json::Value>, StatusCode> {
    if !authorized(&headers, &s) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    if q.contains_key("belajar") {
        let _ = tokio::task::spawn_blocking(|| crate::otak::harian(true)).await;
    }
    Ok(Json(tokio::task::spawn_blocking(crate::otak::json).await.unwrap_or_default()))
}

/// Monitor koneksi (tanpa admin). `?tangkap=1` menangkap paket ±15 dtk (Windows meminta UAC).
async fn paket_get(State(s): State<Arc<Shared>>, headers: HeaderMap, axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>) -> Result<Json<serde_json::Value>, StatusCode> {
    if !authorized(&headers, &s) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    if q.contains_key("tangkap") {
        return Ok(Json(serde_json::json!({"tangkap": crate::paket::tangkap(15).await})));
    }
    Ok(Json(serde_json::to_value(crate::paket::monitor().await).unwrap_or_default()))
}

/// Diagnosa jalur internet untuk dasbor. `?baru=1` memeriksa ulang (±15 dtk); tanpa itu mengembalikan hasil terakhir bila ada.
async fn internet_get(State(s): State<Arc<Shared>>, headers: HeaderMap, axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>) -> Result<Json<serde_json::Value>, StatusCode> {
    if !authorized(&headers, &s) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let r = match crate::internet::terakhir() {
        Some(r) if !q.contains_key("baru") => r,
        _ => crate::internet::periksa(&crate::research::client(10)).await,
    };
    Ok(Json(serde_json::to_value(r).unwrap_or_default()))
}

async fn omni_get(State(s): State<Arc<Shared>>, headers: HeaderMap) -> Result<Json<serde_json::Value>, StatusCode> {
    if !authorized(&headers, &s) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(Json(crate::omniroute::status()))
}

/// Simpan URL/key OmniRoute lalu pindai & uji model otomatis. Key tidak pernah masuk riwayat obrolan.
async fn omni_post(State(s): State<Arc<Shared>>, headers: HeaderMap, Json(r): Json<OmniReq>) -> Result<Json<serde_json::Value>, StatusCode> {
    if !authorized(&headers, &s) || !local_host(&headers) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    if r.hapus {
        crate::omniroute::clear();
        return Ok(Json(serde_json::json!({"ok": true, "pesan": "OmniRoute dihapus."})));
    }
    if let Err(e) = crate::omniroute::save(&r.url, r.key.as_deref()) {
        return Ok(Json(serde_json::json!({"ok": false, "pesan": e.to_string()})));
    }
    let http = crate::research::client(30);
    match crate::omniroute::scan(&http).await {
        Ok(m) => Ok(Json(serde_json::json!({"ok": true, "pesan": format!("Terhubung. {} model teruji; terbaik: {}", m.len(), m[0].model), "models": m}))),
        Err(e) => Ok(Json(serde_json::json!({"ok": false, "pesan": format!("Tersimpan, tetapi pemindaian gagal: {e}")}))),
    }
}

async fn widget_get(State(s): State<Arc<Shared>>, headers: HeaderMap) -> Result<Json<serde_json::Value>, StatusCode> {
    if !authorized(&headers, &s) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let (cpu, ram_u, ram_t, disk_u, disk_t) = {
        let h = s.snap.lock().unwrap_or_else(|e| e.into_inner()).health.clone().unwrap_or_default();
        (h.cpu, h.ram_used_mb, h.ram_total_mb.max(1), h.disk_used_gb, h.disk_total_gb.max(1))
    };
    let mut v = tokio::task::spawn_blocking(crate::widget::data).await.unwrap_or_default();
    v["sistem"] = serde_json::json!({"cpu": cpu, "ram": ram_u as f64 * 100.0 / ram_t as f64, "disk": disk_u as f64 * 100.0 / disk_t as f64});
    Ok(Json(v))
}

async fn widget_post(State(s): State<Arc<Shared>>, headers: HeaderMap, Json(a): Json<serde_json::Value>) -> Result<Json<serde_json::Value>, StatusCode> {
    if !authorized(&headers, &s) || !local_host(&headers) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let pesan = tokio::task::spawn_blocking(move || crate::widget::aksi(&a)).await.unwrap_or_default();
    Ok(Json(serde_json::json!({"pesan": pesan})))
}

async fn stats(State(s): State<Arc<Shared>>, headers: HeaderMap) -> Result<Json<serde_json::Value>, StatusCode> {
    if !authorized(&headers, &s) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    if headers.contains_key("x-nova-ui") {
        if let Ok(mut g) = LAST_UI.lock() {
            *g = Some(Instant::now());
        }
    }
    let h = s.snap.lock().unwrap_or_else(|e| e.into_inner()).health.clone().unwrap_or_default();
    let v = tokio::task::spawn_blocking(move || {
        let p = crate::hewan::pola();
        let streak = crate::assistant::streak_min();
        let cutoff = (chrono::Local::now() - chrono::Duration::days(7)).format("%Y-%m-%d").to_string();
        let (mut days, mut apps): (Vec<(String, i64)>, Vec<(String, i64)>) = (vec![], vec![]);
        if let Ok(c) = crate::assistant::db() {
            if let Ok(mut st) = c.prepare("SELECT day, SUM(secs) FROM usage WHERE day>=?1 GROUP BY day ORDER BY day") {
                days = st.query_map([&cutoff], |r| Ok((r.get(0)?, r.get(1)?))).map(|r| r.flatten().collect()).unwrap_or_default();
            }
            if let Ok(mut st) = c.prepare("SELECT app, SUM(secs) s FROM usage WHERE day>=?1 GROUP BY app ORDER BY s DESC LIMIT 8") {
                apps = st.query_map([&cutoff], |r| Ok((r.get(0)?, r.get(1)?))).map(|r| r.flatten().collect()).unwrap_or_default();
            }
        }
        let (n, acc, base, used) = crate::belajar::stats();
        let examples: i64 = crate::teknik::open().ok().and_then(|c| c.query_row("SELECT COUNT(*) FROM intent_ex", [], |r| r.get(0)).ok()).unwrap_or(0);
        let (gn, gh) = crate::gemini::cache_stats();
        serde_json::json!({
            "per_jam": p.per_jam, "hari": p.hari, "sesi": p.sesi_tipikal, "streak": streak,
            "fatigue": crate::hewan::kelelahan(&p, streak),
            "days": days, "apps": apps,
            "cpu": h.cpu, "cpu_hist": h.history, "ram_used": h.ram_used_mb, "ram_total": h.ram_total_mb.max(1),
            "disk_used": h.disk_used_gb, "disk_total": h.disk_total_gb.max(1), "uptime": h.uptime_min,
            "procs": h.procs.iter().take(6).map(|p| serde_json::json!([p.name, p.cpu, p.mem_mb])).collect::<Vec<_>>(),
            "ads_blocked": h.ads_blocked, "ads_total": h.ads_total, "ads_listed": h.ads_listed,
            "ml": {"samples": n, "acc": acc, "base": base, "used": used},
            "examples": examples, "gcache": [gn, gh], "analitik": crate::analitik::laporan(), "buddy": crate::buddy::kartu(),
        })
    })
    .await
    .unwrap_or_default();
    Ok(Json(v))
}

/// Antarmuka web (jendela aplikasi). Hanya melayani permintaan dari localhost agar token tidak bocor lewat DNS rebinding.
async fn ui(State(s): State<Arc<Shared>>, headers: HeaderMap) -> Response {
    let host = headers.get("host").and_then(|h| h.to_str().ok()).unwrap_or("");
    if !(host.starts_with("127.0.0.1:") || host.starts_with("localhost:")) {
        return StatusCode::FORBIDDEN.into_response();
    }
    Html(UI_HTML.replace("__TOKEN__", &s.token)).into_response()
}

pub fn token_path() -> std::path::PathBuf {
    config::data_dir().join("api.token")
}

pub async fn serve(cfg: Config, port: u16) -> anyhow::Result<()> {
    // Token dipertahankan antar-restart agar ekstensi browser yang sudah terpasang tetap berlaku.
    let token: String = match std::fs::read_to_string(token_path()) {
        Ok(t) if t.trim().len() == 32 => t.trim().to_string(),
        _ => {
            let t: String = (0..32).map(|_| format!("{:x}", rand::thread_rng().gen_range(0..16))).collect();
            std::fs::write(token_path(), &t)?;
            t
        }
    };
    crate::ambien::start();
    let (cmd_tx, cmd_rx) = unbounded_channel();
    let (ev_tx, mut ev_rx) = unbounded_channel();
    tokio::spawn(engine::run(cfg, Tx::new(ev_tx, None), cmd_rx));
    let shared = Arc::new(Shared { cmd: cmd_tx, snap: Mutex::new(Snap::default()), token });
    let s2 = shared.clone();
    tokio::spawn(async move {
        while let Some(e) = ev_rx.recv().await {
            s2.snap.lock().unwrap_or_else(|e| e.into_inner()).apply(&e);
        }
    });
    let app = Router::new().route("/dash", get(dash)).route("/api/stats", get(stats)).route("/api/widget", get(widget_get).post(widget_post)).route("/api/omni", get(omni_get).post(omni_post)).route("/api/internet", get(internet_get)).route("/api/otak", get(otak_get)).route("/api/graf", get(graf_get)).route("/api/alat", get(alat_get)).route("/api/putusan", get(putusan_get).post(putusan_post)).route("/api/bedah", get(bedah_get)).route("/api/ajar", get(ajar_get).post(ajar_post)).route("/api/paket", get(paket_get)).route("/ui", get(ui)).route("/api/pet", get(pet)).route("/api/bec", get(bec_test)).route("/api/cmd", post(post_cmd)).route("/api/ad", post(post_ad)).with_state(shared);
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod ui_tests {
    use super::*;

    fn script_of(html: &str) -> String {
        let a = html.find("<script>").expect("ada <script>") + 8;
        let b = html.rfind("</script>").expect("ada </script>");
        html[a..b].to_string()
    }

    #[test]
    fn pages_have_token_placeholder_and_valid_js() {
        for (name, html) in [("ui", UI_HTML), ("dash", DASH_HTML)] {
            assert_eq!(html.matches("__TOKEN__").count(), 1, "{name}: placeholder token harus tepat satu");
            assert!(html.contains("<title>"), "{name}: judul");
            // sintaks JavaScript diperiksa dengan Node bila terpasang (dilewati bila tidak ada)
            let js = std::env::temp_dir().join(format!("nova_{name}_{}.js", std::process::id()));
            std::fs::write(&js, script_of(html).replace("__TOKEN__", "x")).unwrap();
            if let Ok(o) = std::process::Command::new("node").arg("--check").arg(&js).output() {
                assert!(o.status.success(), "{name}: JS tidak valid: {}", String::from_utf8_lossy(&o.stderr));
            }
            let _ = std::fs::remove_file(&js);
        }
    }

    #[test]
    fn menu_commands_exist_in_ui() {
        // semua perintah menu yang berawalan / harus dikenali engine (tercantum di teks bantuan atau daftar saran)
        let help = [include_str!("engine.rs"), include_str!("engine/slash.rs"), include_str!("engine/teks.rs")].concat();
        for cmd in ["/pola", "/sholat", "/pilih", "/gemini", "/otomatis", "/mulai", "/bantuan", "/latih", "/cadangan", "/build"] {
            assert!(UI_HTML.contains(cmd), "menu UI memuat {cmd}");
            assert!(help.contains(&format!("\"{}\"", cmd.trim_start_matches('/'))) || help.contains(cmd), "engine mengenal {cmd}");
        }
    }
}
