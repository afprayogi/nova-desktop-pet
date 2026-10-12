//! Memori & pencarian TANPA model:
//! - Inverted index: SQLite FTS5 (BM25)           - Dense retrieval: vektor hashing 256-dim (cosine), disimpan di disk
//! - Hybrid search: Reciprocal Rank Fusion        - Cache: LRU+TTL di RAM, memori kerja (ring buffer) di RAM
//! - Graph DB: node/edge SQLite + kNN dari vektor - Memori terstruktur: semantik (KB), episodik (ep), kerja (RAM), profil
//! - RAG & dynamic context injection: context(q) merakit profil + ingatan + pengetahuan + graf untuk prompt/jawaban.
use rusqlite::Connection;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const DIM: usize = 256;

#[derive(Clone)]
pub struct Doc {
    pub id: i64,
    pub title: String,
    pub body: String,
    pub src: String,
}

fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .map(|w| w.to_lowercase())
        .filter(|w| w.len() > 1 && !crate::lex::ada("stop_cari", w))
        .collect()
}

fn fnv(s: &str) -> u64 {
    s.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}

/// Embedding padat tanpa model: feature hashing kata, bigram, dan trigram karakter (tahan salah ketik/imbuhan).
pub fn embed(text: &str) -> Vec<f32> {
    let mut v = vec![0f32; DIM];
    let mut add = |k: &str, w: f32| {
        let h = fnv(k);
        v[(h % DIM as u64) as usize] += if (h >> 40) & 1 == 0 { w } else { -w };
    };
    let ws = words(text);
    for (i, w) in ws.iter().enumerate() {
        add(w, 1.0);
        if let Some(n) = ws.get(i + 1) {
            add(&format!("{w} {n}"), 0.7);
        }
        let padded: Vec<char> = format!("^{w}$").chars().collect();
        for t in padded.windows(3) {
            add(&t.iter().collect::<String>(), 0.3);
        }
    }
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 0.0 {
        v.iter_mut().for_each(|x| *x /= n);
    }
    v
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn from_blob(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

// ---------- Skema, vektor, graf ----------

const EDGES: &[(&str, &str, &str)] = &[
    ("Hukum Ohm", "dasar_dari", "Daya AC satu fasa"),
    ("Daya AC satu fasa", "diperluas_ke", "Daya AC tiga fasa"),
    ("Faktor daya dan kapasitor", "memperbaiki", "Daya AC satu fasa"),
    ("Drop tegangan", "dibatasi_oleh", "Kemampuan hantar arus (KHA) kabel"),
    ("Kemampuan hantar arus (KHA) kabel", "dilindungi_oleh", "MCB, MCCB, ELCB/RCD"),
    ("MCB, MCCB, ELCB/RCD", "bekerja_bersama", "Pentanahan (grounding)"),
    ("Pentanahan (grounding)", "bagian_dari", "Sistem TN, TT, IT"),
    ("Hubung singkat", "menentukan", "MCB, MCCB, ELCB/RCD"),
    ("Motor induksi", "dihidupkan_dengan", "Metode start motor"),
    ("Motor induksi", "dilindungi_oleh", "Proteksi motor"),
    ("Metode start motor", "memakai", "Kontaktor dan relai"),
    ("Trafo", "mempengaruhi", "Hubung singkat"),
    ("Harmonisa", "dikurangi_oleh", "Faktor daya dan kapasitor"),
    ("UPS dan baterai", "terkait", "PLTS (solar PV)"),
    ("PLC dasar", "diprogram_dengan", "Ladder logic"),
    ("PLC dasar", "membaca", "Sinyal analog 4-20 mA"),
    ("Sensor umum", "mengirim_ke", "PLC dasar"),
    ("Komunikasi industri", "menghubungkan", "PLC dasar"),
    ("Ladder logic", "mengendalikan", "Kontaktor dan relai"),
    ("ADC dan resolusi", "digunakan_pada", "Sensor umum"),
    ("Op-amp", "dipakai_pada", "Filter RC dan frekuensi cutoff"),
    ("Resistor, kapasitor, induktor", "membentuk", "Filter RC dan frekuensi cutoff"),
    ("LED dan resistor pembatas", "menerapkan", "Hukum Ohm"),
    ("Dioda dan transistor", "dipakai_pada", "PWM"),
    ("Keselamatan kerja listrik", "wajib_untuk", "Pentanahan (grounding)"),
    ("Warna kabel instalasi", "bagian_dari", "Keselamatan kerja listrik"),
    ("IP rating", "menentukan", "Keselamatan kerja listrik"),
    ("Resistivitas bahan", "dipakai_pada", "Drop tegangan"),
    ("Konversi satuan daya", "dipakai_pada", "Motor induksi"),
];

/// Dipanggil dari teknik::open(): tabel tambahan, isi vektor yang belum ada, dan graf awal.
pub fn init(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS kb_vec(id INTEGER PRIMARY KEY, v BLOB);
         CREATE TABLE IF NOT EXISTS edge(a TEXT, rel TEXT, b TEXT, PRIMARY KEY(a,rel,b));
         CREATE VIRTUAL TABLE IF NOT EXISTS ep USING fts5(text, kind UNINDEXED, tokenize='unicode61');",
    )?;
    let missing: Vec<(i64, String)> = {
        let mut st = c.prepare("SELECT rowid, title||' '||body FROM kb WHERE rowid NOT IN (SELECT id FROM kb_vec) LIMIT 3000")?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.flatten().collect()
    };
    if !missing.is_empty() {
        let tx = c.unchecked_transaction()?;
        for (id, t) in &missing {
            tx.execute("INSERT OR REPLACE INTO kb_vec VALUES(?1,?2)", (id, to_blob(&embed(t))))?;
        }
        tx.commit()?;
    }
    c.execute_batch("CREATE TABLE IF NOT EXISTS intent_ex(label TEXT, text TEXT);")?;
    static GRAPH_DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if GRAPH_DONE.load(std::sync::atomic::Ordering::Relaxed) && missing.is_empty() {
        return Ok(());
    }
    GRAPH_DONE.store(true, std::sync::atomic::Ordering::Relaxed);
    // Relasi kurasi (idempoten).
    {
        let tx = c.unchecked_transaction()?;
        for (a, r, b) in EDGES.iter().chain(crate::kode::EDGES.iter()) {
            tx.execute("INSERT OR IGNORE INTO edge VALUES(?1,?2,?3)", (a, r, b))?;
        }
        tx.commit()?;
    }
    // Graf kNN otomatis untuk catatan bawaan/pemrograman yang belum punya relasi.
    let lonely: Vec<String> = {
        let mut st = c.prepare(
            "SELECT title FROM kb WHERE src IN ('bawaan','pemrograman') AND title NOT IN (SELECT a FROM edge WHERE rel='mirip') LIMIT 200",
        )?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        rows.flatten().collect()
    };
    if !lonely.is_empty() {
        let items: Vec<(String, Vec<f32>)> = {
            let mut st = c.prepare("SELECT k.title, v.v FROM kb k JOIN kb_vec v ON v.id = k.rowid WHERE k.src IN ('bawaan','pemrograman')")?;
            let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, from_blob(&r.get::<_, Vec<u8>>(1)?))))?;
            rows.flatten().collect()
        };
        let tx = c.unchecked_transaction()?;
        for (t, v) in items.iter().filter(|(t, _)| lonely.contains(t)) {
            let mut sims: Vec<(f32, &String)> = items.iter().filter(|(t2, _)| t2 != t).map(|(t2, v2)| (dot(v, v2), t2)).collect();
            sims.sort_by(|a, b| b.0.total_cmp(&a.0));
            let mut any = false;
            for (_, t2) in sims.into_iter().take(2).filter(|x| x.0 > 0.12) {
                tx.execute("INSERT OR IGNORE INTO edge VALUES(?1,'mirip',?2)", (t, t2))?;
                any = true;
            }
            if !any {
                tx.execute("INSERT OR IGNORE INTO edge VALUES(?1,'mirip',?1)", [t])?; // penanda agar tidak dihitung ulang
            }
        }
        tx.commit()?;
    }
    Ok(())
}

/// Entitas terkait (graf, 2 lompatan, dua arah).
pub fn related(title: &str, limit: usize) -> Vec<String> {
    let Ok(c) = crate::teknik::open() else { return vec![] };
    let sql = "WITH RECURSIVE r(n,d) AS (
                 SELECT ?1, 0
                 UNION
                 SELECT CASE WHEN e.a = r.n THEN e.b ELSE e.a END, r.d+1 FROM edge e JOIN r ON e.a = r.n OR e.b = r.n WHERE r.d < 2
               ) SELECT n, MIN(d) md FROM r WHERE n <> ?1 GROUP BY n ORDER BY md, n LIMIT ?2";
    c.prepare(sql)
        .and_then(|mut st| st.query_map((title, limit as i64), |r| r.get::<_, String>(0)).map(|r| r.flatten().collect()))
        .unwrap_or_default()
}

// ---------- Cache LRU (RAM) ----------

struct Cache {
    map: HashMap<String, (Instant, Vec<Doc>)>,
    order: VecDeque<String>,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);
const CAP: usize = 64;
const TTL: Duration = Duration::from_secs(300);

fn cache_get(k: &str) -> Option<Vec<Doc>> {
    let mut g = CACHE.lock().ok()?;
    let c = g.as_mut()?;
    match c.map.get(k) {
        Some((t, d)) if t.elapsed() < TTL => {
            let d = d.clone();
            c.order.retain(|x| x != k);
            c.order.push_back(k.to_string());
            Some(d)
        }
        _ => None,
    }
}

fn cache_put(k: String, d: Vec<Doc>) {
    if let Ok(mut g) = CACHE.lock() {
        let c = g.get_or_insert_with(|| Cache { map: HashMap::new(), order: VecDeque::new() });
        c.order.retain(|x| *x != k);
        c.order.push_back(k.clone());
        c.map.insert(k, (Instant::now(), d));
        while c.order.len() > CAP {
            if let Some(old) = c.order.pop_front() {
                c.map.remove(&old);
            }
        }
    }
}

pub fn clear_cache() {
    if let Ok(mut g) = CACHE.lock() {
        *g = None;
    }
}

// ---------- Hybrid search (sparse BM25 + dense cosine, RRF) ----------

pub fn hybrid(q: &str, k: usize) -> Vec<Doc> {
    hybrid_v(q, k, None)
}

/// Seperti `hybrid`, dengan embedding makna pertanyaan (`nv`, ternormalisasi) bila tersedia: ranking ketiga dari tabel `kb_emb`.
pub fn hybrid_v(q: &str, k: usize, nv: Option<&[f32]>) -> Vec<Doc> {
    let key = format!("{k}|{}|{}", nv.is_some(), q.to_lowercase());
    if let Some(d) = cache_get(&key) {
        return d;
    }
    let Ok(c) = crate::teknik::open() else { return vec![] };
    let mut score: HashMap<i64, f32> = HashMap::new();

    // Sparse: indeks terbalik FTS5 + BM25.
    let t = crate::teknik::terms(q);
    if !t.is_empty() {
        let m = t.iter().map(|w| format!("\"{w}\"")).collect::<Vec<_>>().join(" OR ");
        if let Ok(mut st) = c.prepare("SELECT rowid FROM kb WHERE kb MATCH ?1 ORDER BY bm25(kb, 5.0, 1.0) LIMIT 20") {
            if let Ok(rows) = st.query_map([m], |r| r.get::<_, i64>(0)) {
                for (rank, id) in rows.flatten().enumerate() {
                    *score.entry(id).or_default() += 1.0 / (60.0 + rank as f32);
                }
            }
        }
    }
    // Dense: cosine terhadap semua vektor di disk (dibaca bertahap).
    let qv = embed(q);
    let mut dense: Vec<(f32, i64)> = vec![];
    if let Ok(mut st) = c.prepare("SELECT id, v FROM kb_vec") {
        if let Ok(rows) = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?))) {
            for (id, b) in rows.flatten() {
                let s = dot(&qv, &from_blob(&b));
                if s > 0.08 {
                    dense.push((s, id));
                }
            }
        }
    }
    dense.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (rank, (_, id)) in dense.into_iter().take(20).enumerate() {
        *score.entry(id).or_default() += 1.0 / (60.0 + rank as f32);
    }

    // Makna (nomic-embed-text): bobot lebih besar karena lintas kata/bahasa.
    if let Some(nv) = nv {
        let mut sem: Vec<(f32, i64)> = vec![];
        if let Ok(mut st) = c.prepare("SELECT id, v FROM kb_emb") {
            if let Ok(rows) = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?))) {
                for (id, b) in rows.flatten() {
                    let s = dot(nv, &crate::vektor::dari_blob(&b));
                    if s > 0.45 {
                        sem.push((s, id));
                    }
                }
            }
        }
        sem.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (rank, (_, id)) in sem.into_iter().take(20).enumerate() {
            *score.entry(id).or_default() += 1.6 / (60.0 + rank as f32);
        }
    }

    let mut ranked: Vec<(i64, f32)> = score.into_iter().collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
    let docs: Vec<Doc> = ranked
        .into_iter()
        .take(k)
        .filter_map(|(id, _)| {
            c.query_row("SELECT rowid, title, body, src FROM kb WHERE rowid=?1", [id], |r| {
                Ok(Doc { id: r.get(0)?, title: r.get(1)?, body: r.get(2)?, src: r.get(3)? })
            })
            .ok()
        })
        .collect();
    // jawaban yang sudah dinilai benar oleh Bos didahulukan
    let mut docs = docs;
    docs.sort_by_key(|d| !d.src.starts_with("terverifikasi-"));
    cache_put(key, docs.clone());
    docs
}

// ---------- Memori kerja (RAM) & episodik (disk) ----------

static WORK: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());

pub fn work_push(s: &str) {
    if let Ok(mut w) = WORK.lock() {
        w.push_back(s.chars().take(160).collect());
        while w.len() > 30 {
            w.pop_front();
        }
    }
}

pub fn work_recent(n: usize) -> Vec<String> {
    WORK.lock().map(|w| w.iter().rev().take(n).cloned().collect()).unwrap_or_default()
}

pub fn episode(kind: &str, text: &str) {
    work_push(&format!("{kind}: {text}"));
    if let Ok(c) = crate::teknik::open() {
        let _ = c.execute("INSERT INTO ep(text, kind) VALUES(?1, ?2)", (text, kind));
        let _ = c.execute("DELETE FROM ep WHERE rowid < (SELECT MAX(rowid) - 2000 FROM ep)", []);
    }
}

pub fn recall(q: &str, k: usize) -> Vec<String> {
    let t = crate::teknik::terms(q);
    if t.is_empty() {
        return vec![];
    }
    let m = t.iter().map(|w| format!("\"{w}\"")).collect::<Vec<_>>().join(" OR ");
    let Ok(c) = crate::teknik::open() else { return vec![] };
    c.prepare("SELECT kind||': '||text FROM ep WHERE ep MATCH ?1 ORDER BY bm25(ep) LIMIT ?2")
        .and_then(|mut st| st.query_map((m, k as i64), |r| r.get::<_, String>(0)).map(|r| r.flatten().collect()))
        .unwrap_or_default()
}

// ---------- Profil pengguna (cache 60 dtk) ----------

static PROFILE: Mutex<Option<(Instant, Vec<String>)>> = Mutex::new(None);

pub fn profile() -> Vec<String> {
    if let Ok(g) = PROFILE.lock() {
        if let Some((t, p)) = g.as_ref() {
            if t.elapsed() < Duration::from_secs(60) {
                return p.clone();
            }
        }
    }
    let mut p = crate::assistant::profile_facts();
    if let Ok(c) = crate::teknik::open() {
        let texts: Vec<String> = c
            .prepare("SELECT text FROM ep WHERE kind='tanya' ORDER BY rowid DESC LIMIT 200")
            .and_then(|mut st| st.query_map([], |r| r.get::<_, String>(0)).map(|r| r.flatten().collect()))
            .unwrap_or_default();
        let mut f: HashMap<String, u32> = HashMap::new();
        for t in &texts {
            for w in crate::teknik::terms(t) {
                if w.len() > 3 {
                    *f.entry(w).or_default() += 1;
                }
            }
        }
        let mut top: Vec<_> = f.into_iter().collect();
        top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        if !top.is_empty() {
            p.push(format!("Topik yang sering ditanyakan: {}", top.iter().take(4).map(|x| x.0.as_str()).collect::<Vec<_>>().join(", ")));
        }
    }
    if let Ok(mut g) = PROFILE.lock() {
        *g = Some((Instant::now(), p.clone()));
    }
    p
}

// ---------- RAG: perakitan konteks dinamis ----------

/// Entitas graf yang terakhir dipakai menjawab (untuk disorot di tab Otak dasbor).
pub static AKTIF: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Ingatan yang dipakai untuk kalimat ini, sebagai satu baris obrolan ("🧠 Ingatan terkait: …"); kosong bila tidak ada yang relevan.
pub fn dipakai(q: &str) -> String {
    let kata: Vec<String> = crate::teknik::terms(q).into_iter().filter(|w| w.len() > 3).collect();
    let docs: Vec<Doc> = hybrid(q, 3).into_iter().filter(|d| d.src != "bawaan" && !d.title.trim_start().starts_with("==") && d.body.chars().count() > 60 && { let t = format!("{} {}", d.title, d.body).to_lowercase(); kata.iter().any(|w| t.contains(w.as_str())) }).collect();
    let ql = q.to_lowercase();
    let (gd, rel) = crate::graf::recall(q, 3);
    let rel: Vec<String> = rel.into_iter().filter(|r| !ql.contains(&r.to_lowercase()) && !["halo", "nova", "carikan", "coba", "buka", "tolong", "bisa", "cari", "tanya", "gemini", "pakai", "jawab", "internet", "kamu", "saya", "aku", "mau", "lagi", "video", "lagu"].contains(&r.to_lowercase().as_str())).collect(); // entitas yang hanya mengulang kalimat tidak informatif
    if let Ok(mut a) = AKTIF.lock() {
        *a = rel.iter().take(24).cloned().collect();
    }
    let mut bagian: Vec<String> = docs.iter().map(|d| {
        let sumber = d.src.split(':').next().unwrap_or("").chars().take(14).collect::<String>();
        format!("{} ({})", d.title.chars().take(40).collect::<String>(), sumber)
    }).collect();
    bagian.extend(rel.iter().take(3).map(|r| format!("#{r}")));
    if bagian.is_empty() { String::new() } else { format!("🧠 Ingatan terkait: {}", bagian.join(" · ")) }
}

/// Konteks untuk prompt model (injeksi dinamis) maupun jawaban tanpa model.
pub fn context(q: &str) -> String {
    let mut s = String::new();
    let p = profile();
    if !p.is_empty() {
        s.push_str(&format!("PROFIL BOS:\n- {}\n", p.join("\n- ")));
    }
    let w = work_recent(3);
    if !w.is_empty() {
        s.push_str(&format!("AKTIVITAS TERBARU:\n- {}\n", w.join("\n- ")));
    }
    let docs = hybrid(q, 3);
    if !docs.is_empty() {
        s.push_str("PENGETAHUAN RELEVAN:\n");
        for d in &docs {
            s.push_str(&format!("- {} [{}]: {}\n", d.title, d.src, d.body.chars().take(220).collect::<String>()));
        }
        let rel = related(&docs[0].title, 5);
        if !rel.is_empty() {
            s.push_str(&format!("TERKAIT (graf): {}\n", rel.join("; ")));
        }
    }
    let (gd, grel) = crate::graf::recall(q, 3);
    if !gd.is_empty() {
        s.push_str("INGATAN JANGKA PANJANG (graf):\n");
        for d in &gd {
            s.push_str(&format!("- [{}] {}\n", d.kind, d.text.chars().take(160).collect::<String>().replace('\n', " ")));
        }
        if !grel.is_empty() {
            s.push_str(&format!("ENTITAS TERKAIT: {}\n", grel.join(", ")));
        }
    }
    let r = recall(q, 2);
    if !r.is_empty() {
        s.push_str(&format!("INGATAN SERUPA:\n- {}\n", r.join("\n- ")));
    }
    s.chars().take(1800).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn embedding_is_similar_for_related_text() {
        let a = embed("pentanahan grounding bumi");
        let b = embed("Pentanahan (grounding) arus gangguan ke tanah");
        let c = embed("resep nasi goreng pedas");
        assert!(dot(&a, &b) > dot(&a, &c));
    }
    #[test]
    fn hybrid_finds_with_typo_and_graph_links() {
        let d = hybrid("kapasitor perbaikan faktor daya", 3);
        assert!(d.iter().any(|x| x.title.contains("Faktor daya")));
        let typo = hybrid("pentanahann gounding", 8); 
        assert!(typo.iter().any(|x| x.title.contains("Pentanahan")));
        assert!(!related("Motor induksi", 5).is_empty());
    }
    #[test]
    fn working_memory_and_cache() {
        work_push("a");
        work_push("b");
        assert_eq!(work_recent(1), vec!["b".to_string()]);
        cache_put("k".into(), vec![]);
        assert!(cache_get("k").is_some());
    }
}
