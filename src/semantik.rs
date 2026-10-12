//! Pemahaman perintah secara SEMANTIK (metode "semantic router" yang umum dipakai agen modern): kalimat Bos dan contoh
//! niat diubah menjadi vektor embedding (Gemini), lalu niat dipilih dari contoh yang paling mirip (cosine). Lebih tahan
//! terhadap variasi kalimat daripada TF-IDF. Vektor contoh disimpan lokal; hanya kalimat Bos (satu panggilan kecil, dicache)
//! yang dikirim, dan hanya bila Gemini aktif. Hasil hanya dipakai untuk niat yang jelas (bukan tanya/ngobrol bebas).
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;

const MODEL: &str = "gemini-embedding-001";
const DIM: usize = 256;

pub const CONTOH_BAWAAN: &str = include_str!("../data/contoh_niat.json");

/// Contoh kalimat per niat: bawaan (data/contoh_niat.json) + tambahan Bos (otak/contoh_niat.json), ditambah contoh yang dipelajari dari pemakaian.
fn contoh_niat() -> Vec<(String, String)> {
    crate::data::lapis("contoh_niat.json", CONTOH_BAWAAN)
        .as_object()
        .map(|o| o.iter().flat_map(|(l, ex)| ex.as_array().into_iter().flatten().filter_map(|e| e.as_str().map(|e| (l.clone(), e.to_string()))).collect::<Vec<_>>()).collect())
        .unwrap_or_default()
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn norm(mut v: Vec<f32>) -> Vec<f32> {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-6);
    v.iter_mut().for_each(|x| *x /= n);
    v
}

fn to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn from_blob(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

fn db() -> Option<rusqlite::Connection> {
    let c = crate::assistant::db().ok()?;
    c.execute_batch("CREATE TABLE IF NOT EXISTS emb(k TEXT PRIMARY KEY, label TEXT, v BLOB, ts INT);").ok()?;
    Some(c)
}

async fn embed(http: &reqwest::Client, texts: &[String]) -> Result<Vec<Vec<f32>>> {
    // 1) model lokal (offline, tanpa kuota); 2) Gemini
    if let Ok(v) = crate::lokal::embed(http, texts).await {
        set_used(crate::lokal::EMBED_MODEL);
        return Ok(v.into_iter().map(norm).collect());
    }
    let k = crate::gemini::api_key().ok_or_else(|| anyhow!("tidak ada model embedding (pasang model lokal atau key Gemini)"))?;
    let mut last = String::new();
    // Coba model embedding bergantian: kuota tiap model terpisah di tier gratis.
    for m in [MODEL, "gemini-embedding-2", "text-embedding-004", "embedding-001"] {
        let mut r = json!({"taskType": "SEMANTIC_SIMILARITY"});
        if m.starts_with("gemini-embedding") {
            r["outputDimensionality"] = json!(DIM);
        }
        let reqs: Vec<Value> = texts
            .iter()
            .map(|t| {
                let mut x = r.clone();
                x["model"] = json!(format!("models/{m}"));
                x["content"] = json!({"parts": [{"text": t}]});
                x
            })
            .collect();
        let resp = http
            .post(format!("https://generativelanguage.googleapis.com/v1beta/models/{m}:batchEmbedContents"))
            .header("x-goog-api-key", &k)
            .timeout(Duration::from_secs(30))
            .json(&json!({"requests": reqs}))
            .send()
            .await?;
        let st = resp.status();
        let v: Value = resp.json().await.unwrap_or(Value::Null);
        if !st.is_success() {
            last = format!("{m}: HTTP {} {}", st.as_u16(), v["error"]["message"].as_str().unwrap_or("").chars().take(90).collect::<String>());
            continue;
        }
        let out: Vec<Vec<f32>> = v["embeddings"].as_array().map(|a| a.iter().map(|e| norm(e["values"].as_array().map(|x| x.iter().filter_map(|f| f.as_f64().map(|f| f as f32)).collect()).unwrap_or_default())).collect()).unwrap_or_default();
        if out.len() == texts.len() {
            set_used(m);
            return Ok(out);
        }
    }
    bail!("embedding gagal ({last})")
}

fn set_used(m: &str) {
    if let Some(c) = db() {
        // vektor dari model berbeda tidak bisa dibandingkan: bila model berganti, kosongkan indeks.
        let _ = c.execute_batch("CREATE TABLE IF NOT EXISTS emb_meta(k TEXT PRIMARY KEY, v TEXT);");
        let cur: Option<String> = c.query_row("SELECT v FROM emb_meta WHERE k='model'", [], |r| r.get(0)).ok();
        if cur.as_deref() != Some(m) {
            let _ = c.execute("DELETE FROM emb", []);
            let _ = c.execute("INSERT OR REPLACE INTO emb_meta VALUES('model', ?1)", [m]);
        }
    }
}

/// Pastikan semua contoh (bawaan + dipelajari) punya vektor tersimpan.
async fn ensure_index(http: &reqwest::Client) -> Result<()> {
    let c = db().ok_or_else(|| anyhow!("database tidak tersedia"))?;
    let mut all: Vec<(String, String)> = contoh_niat().into_iter().collect();
    if let Ok(k) = crate::teknik::open() {
        if let Ok(mut st) = k.prepare("SELECT label, text FROM intent_ex") {
            if let Ok(rows) = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))) {
                all.extend(rows.flatten());
            }
        }
    }
    let have: std::collections::HashSet<String> = c.prepare("SELECT k FROM emb WHERE label<>''").ok().and_then(|mut s| s.query_map([], |r| r.get::<_, String>(0)).ok().map(|r| r.flatten().collect())).unwrap_or_default();
    let todo: Vec<(String, String)> = all.into_iter().filter(|(_, t)| !have.contains(&t.to_lowercase())).collect();
    for chunk in todo.chunks(100).take(4) {
        let texts: Vec<String> = chunk.iter().map(|(_, t)| t.to_lowercase()).collect();
        let vs = embed(http, &texts).await?;
        for ((l, _), (t, v)) in chunk.iter().zip(texts.iter().zip(vs)) {
            let _ = c.execute("INSERT OR REPLACE INTO emb(k,label,v,ts) VALUES(?1,?2,?3,strftime('%s','now'))", rusqlite::params![t, l, to_blob(&v)]);
        }
    }
    Ok(())
}

/// Skor tertinggi per niat untuk kalimat `text` (urut menurun).
pub async fn scores(http: &reqwest::Client, text: &str) -> Result<Vec<(String, f32)>> {
    ensure_index(http).await?;
    let c = db().ok_or_else(|| anyhow!("database tidak tersedia"))?;
    let q = text.to_lowercase();
    let qv: Vec<f32> = match c.query_row("SELECT v FROM emb WHERE k=?1 AND label=''", [&q], |r| r.get::<_, Vec<u8>>(0)) {
        Ok(b) => from_blob(&b),
        Err(_) => {
            let v = embed(http, &[q.clone()]).await?.remove(0);
            let _ = c.execute("INSERT OR REPLACE INTO emb(k,label,v,ts) VALUES(?1,'',?2,strftime('%s','now'))", rusqlite::params![q, to_blob(&v)]);
            let _ = c.execute("DELETE FROM emb WHERE label='' AND ts < strftime('%s','now') - 30*86400", []);
            v
        }
    };
    let neg: std::collections::HashSet<String> = crate::ml::negatives(text);
    let mut best: HashMap<String, f32> = HashMap::new();
    let mut st = c.prepare("SELECT label, v FROM emb WHERE label<>''")?;
    for row in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?)))?.flatten() {
        if neg.contains(&row.0) {
            continue;
        }
        let s = cosine(&qv, &from_blob(&row.1));
        let e = best.entry(row.0).or_insert(f32::MIN);
        if s > *e {
            *e = s;
        }
    }
    let mut v: Vec<(String, f32)> = best.into_iter().collect();
    v.sort_by(|a, b| b.1.total_cmp(&a.1));
    Ok(v)
}

/// Niat yang jelas (skor tinggi dan unggul dari niat lain), atau None.
pub async fn classify(http: &reqwest::Client, text: &str) -> Option<(String, f32)> {
    let v = scores(http, text).await.ok()?;
    let (top, second) = (v.first()?, v.get(1).map(|x| x.1).unwrap_or(0.0));
    if !(top.1 >= 0.74 && top.1 - second >= 0.02) {
        return None;
    }
    // Pagar leksikal: embedding bisa "mirip" secara semu; kalimat harus berbagi minimal satu kata isi dengan contoh niat itu.
    let words = |t: &str| -> std::collections::HashSet<String> { t.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| w.len() > 2 && !crate::lex::ada("stop_niat", w)).map(String::from).collect() };
    let q = words(text);
    let c = db()?;
    let label = top.0.clone();
    let mut st = c.prepare("SELECT k FROM emb WHERE label=?1").ok()?;
    let share = st.query_map([&label], |r| r.get::<_, String>(0)).ok()?.flatten().any(|k| !words(&k).is_disjoint(&q));
    share.then(|| top.clone())
}
