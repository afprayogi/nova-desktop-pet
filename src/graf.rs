//! Graphify: memori jangka panjang berbentuk GRAF pengetahuan, murni lokal tanpa model.
//! Setiap pesan/berkas diekstrak menjadi entitas (kata kunci, nama, berkas, domain, identifier kode); entitas yang muncul bersama
//! dihubungkan dengan bobot, dan potongan teks asal disimpan sebagai "dokumen" yang tertaut ke simpul. Pemanggilan ingatan memakai
//! spreading activation (mirip personalized PageRank) dari entitas pertanyaan sehingga konteks percakapan berhari-hari bisa
//! diingat kembali, dengan peluruhan waktu (setengah-umur 30 hari). Juga bisa "graphify" folder kode/dokumen dan mengekspor
//! peta graf interaktif ke satu berkas HTML (opsional, `/graf lihat`).
use regex::Regex;
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::{Duration, Instant};

const STOP: &[&str] = &[
    "yang", "dan", "di", "ke", "dari", "ini", "itu", "untuk", "pada", "dengan", "adalah", "dalam", "atau", "juga", "akan", "oleh", "saya", "aku", "kamu", "kita", "tolong",
    "bisa", "tidak", "bukan", "sudah", "belum", "lagi", "agar", "supaya", "karena", "jika", "kalau", "maka", "saat", "ketika", "apa", "bagaimana", "kenapa", "berapa", "mana",
    "the", "and", "for", "that", "this", "with", "from", "have", "are", "was", "were", "will", "would", "could", "should", "what", "when", "where", "which", "there", "their",
    "about", "into", "than", "then", "them", "they", "your", "you", "can", "not", "but", "all", "any", "has", "had", "its", "our", "out", "use", "using", "just", "like",
    "bahas", "membahas", "dibahas", "bicara", "bicarakan", "soal", "ingat", "pernah", "kemarin", "sebelumnya", "ceritakan", "jelaskan", "tentang", "seperti", "setiap", "semua", "banyak", "sedikit", "lebih", "paling", "sangat", "banget", "dong", "deh", "nih", "kok", "ya", "yah", "nya", "tadi", "nanti",
];

fn open() -> rusqlite::Result<Connection> {
    let c = crate::teknik::open()?;
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS g_node(id INTEGER PRIMARY KEY, name TEXT UNIQUE, kind TEXT, weight REAL, first_ts INT, last_ts INT);
         CREATE TABLE IF NOT EXISTS g_edge(a INT, b INT, w REAL, last_ts INT, PRIMARY KEY(a,b));
         CREATE TABLE IF NOT EXISTS g_doc(id INTEGER PRIMARY KEY, kind TEXT, ref TEXT, text TEXT, ts INT);
         CREATE TABLE IF NOT EXISTS g_link(node INT, doc INT, PRIMARY KEY(node,doc));
         CREATE INDEX IF NOT EXISTS g_edge_b ON g_edge(b);
         CREATE INDEX IF NOT EXISTS g_link_doc ON g_link(doc);",
    )?;
    Ok(c)
}

fn now() -> i64 {
    chrono::Local::now().timestamp()
}

#[derive(Clone, Debug, PartialEq)]
pub struct Ent {
    pub name: String,
    pub kind: &'static str,
}

/// Ekstrak entitas penting dari teks (maks `max`).
pub fn extract(text: &str, max: usize) -> Vec<Ent> {
    let mut out: Vec<Ent> = vec![];
    let mut seen: HashSet<String> = HashSet::new();
    let mut add = |name: String, kind: &'static str, out: &mut Vec<Ent>| {
        let n = name.trim().to_lowercase();
        if n.len() >= 3 && n.len() <= 48 && !STOP.contains(&n.as_str()) && seen.insert(n.clone()) {
            out.push(Ent { name: n, kind });
        }
    };
    let file_re = Regex::new(r"(?i)\b[\w\-]+\.(?:rs|py|js|ts|tsx|jsx|json|md|pdf|docx?|xlsx?|pptx?|toml|html|css|cpp|hpp|c|h|java|go|sql|txt|ps1|bat|csv|ini)\b").unwrap();
    for m in file_re.find_iter(text) {
        add(m.as_str().to_string(), "berkas", &mut out);
    }
    let dom_re = Regex::new(r"(?i)\b(?:[a-z0-9-]+\.)+(?:com|org|net|id|io|dev|co|ai|edu|gov)\b").unwrap();
    for m in dom_re.find_iter(text) {
        if !m.as_str().contains("..") {
            add(m.as_str().to_string(), "domain", &mut out);
        }
    }
    let ident_re = Regex::new(r"\b[a-z][a-z0-9]*(?:_[a-z0-9]+)+\b|\b[A-Z][a-z0-9]+(?:[A-Z][a-z0-9]+)+\b").unwrap();
    for m in ident_re.find_iter(text) {
        add(m.as_str().to_string(), "kode", &mut out);
    }
    // Nama diri: kata berhuruf kapital yang bukan awal kalimat.
    let words: Vec<&str> = text.split(|c: char| !c.is_alphanumeric() && c != '\'').filter(|w| !w.is_empty()).collect();
    let mut prev_end = true;
    for w in &words {
        let cap = w.chars().next().is_some_and(|c| c.is_uppercase()) && w.len() >= 3 && w.chars().skip(1).any(|c| c.is_lowercase());
        if cap && !prev_end {
            add(w.to_string(), "nama", &mut out);
        }
        prev_end = false;
    }
    // Kata kunci biasa menurut frekuensi (dan panjang).
    let mut freq: HashMap<String, usize> = HashMap::new();
    for w in &words {
        let l = w.to_lowercase();
        if l.len() >= 4 && l.chars().all(|c| c.is_alphabetic()) && !STOP.contains(&l.as_str()) {
            *freq.entry(l).or_default() += 1;
        }
    }
    let mut kw: Vec<(String, usize)> = freq.into_iter().collect();
    kw.sort_by(|a, b| b.1.cmp(&a.1).then(b.0.len().cmp(&a.0.len())).then(a.0.cmp(&b.0)));
    for (w, _) in kw {
        if out.len() >= max {
            break;
        }
        add(w, "kata", &mut out);
    }
    out.truncate(max);
    out
}

fn upsert_node(c: &Connection, e: &Ent, ts: i64, w: f64) -> Option<i64> {
    c.execute(
        "INSERT INTO g_node(name,kind,weight,first_ts,last_ts) VALUES(?1,?2,?3,?4,?4) ON CONFLICT(name) DO UPDATE SET weight=weight+?3, last_ts=?4",
        (&e.name, e.kind, w, ts),
    )
    .ok()?;
    c.query_row("SELECT id FROM g_node WHERE name=?1", [&e.name], |r| r.get(0)).ok()
}

fn upsert_edge(c: &Connection, a: i64, b: i64, w: f64, ts: i64) {
    if a == b {
        return;
    }
    let (x, y) = if a < b { (a, b) } else { (b, a) };
    let _ = c.execute("INSERT INTO g_edge VALUES(?1,?2,?3,?4) ON CONFLICT(a,b) DO UPDATE SET w=w+?3, last_ts=?4", (x, y, w, ts));
}

/// Simpan satu "dokumen" (potongan teks) dan hubungkan entitasnya di graf. Mengembalikan jumlah entitas.
pub fn ingest(kind: &str, text: &str) -> usize {
    ingest_ref(kind, "", text)
}

pub fn ingest_ref(kind: &str, r: &str, text: &str) -> usize {
    let ents = extract(text, 12);
    if ents.is_empty() || text.trim().len() < 8 {
        return 0;
    }
    let Ok(mut c) = open() else { return 0 };
    let Ok(tx) = c.transaction() else { return 0 };
    let ts = now();
    let snippet: String = text.trim().chars().take(300).collect();
    if tx.execute("INSERT INTO g_doc(kind,ref,text,ts) VALUES(?1,?2,?3,?4)", (kind, r, &snippet, ts)).is_err() {
        return 0;
    }
    let doc = tx.last_insert_rowid();
    let ids: Vec<i64> = ents.iter().filter_map(|e| upsert_node(&tx, e, ts, 1.0)).collect();
    for id in &ids {
        let _ = tx.execute("INSERT OR IGNORE INTO g_link VALUES(?1,?2)", (id, doc));
    }
    for i in 0..ids.len() {
        for j in i + 1..ids.len() {
            upsert_edge(&tx, ids[i], ids[j], 1.0, ts);
        }
    }
    let _ = tx.commit();
    ids.len()
}

#[derive(Clone, Debug)]
pub struct Recalled {
    pub ts: i64,
    pub kind: String,
    pub text: String,
    pub score: f64,
}

fn find_nodes(c: &Connection, ents: &[Ent]) -> Vec<i64> {
    let mut ids = vec![];
    for e in ents {
        if let Ok(id) = c.query_row("SELECT id FROM g_node WHERE name=?1", [&e.name], |r| r.get::<_, i64>(0)) {
            ids.push(id);
        } else if e.name.len() >= 5 {
            // awalan (imbuhan: 'baterai' ~ 'baterainya')
            let pat = format!("{}%", &e.name[..e.name.len() - 1]);
            if let Ok(id) = c.query_row("SELECT id FROM g_node WHERE name LIKE ?1 ORDER BY weight DESC LIMIT 1", [pat], |r| r.get::<_, i64>(0)) {
                ids.push(id);
            }
        }
    }
    ids
}

fn neighbors(c: &Connection, n: i64, limit: usize) -> Vec<(i64, f64)> {
    c.prepare("SELECT b, w FROM g_edge WHERE a=?1 UNION ALL SELECT a, w FROM g_edge WHERE b=?1 ORDER BY 2 DESC LIMIT ?2")
        .and_then(|mut st| st.query_map((n, limit as i64), |r| Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?))).map(|r| r.flatten().collect()))
        .unwrap_or_default()
}

/// Spreading activation dari simpul benih; mengembalikan aktivasi per simpul.
fn activate(c: &Connection, seeds: &[i64]) -> HashMap<i64, f64> {
    let mut act: HashMap<i64, f64> = seeds.iter().map(|s| (*s, 1.0)).collect();
    for _ in 0..3 {
        let mut next = act.clone();
        for (n, a) in act.iter() {
            let nb = neighbors(c, *n, 25);
            let total: f64 = nb.iter().map(|x| x.1).sum();
            if total <= 0.0 {
                continue;
            }
            for (m, w) in nb {
                *next.entry(m).or_default() += 0.5 * a * w / total;
            }
        }
        act = next;
    }
    act
}

fn node_name(c: &Connection, id: i64) -> String {
    c.query_row("SELECT name FROM g_node WHERE id=?1", [id], |r| r.get(0)).unwrap_or_default()
}

/// Ingatan jangka panjang untuk sebuah pertanyaan: dokumen paling relevan (dengan peluruhan waktu) + entitas terkait.
pub fn recall(query: &str, k: usize) -> (Vec<Recalled>, Vec<String>) {
    let Ok(c) = open() else { return (vec![], vec![]) };
    let ents = extract(query, 8);
    let seeds = find_nodes(&c, &ents);
    if seeds.is_empty() {
        return (vec![], vec![]);
    }
    let act = activate(&c, &seeds);
    let mut ranked: Vec<(i64, f64)> = act.iter().map(|(k, v)| (*k, *v)).collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
    let seed_set: HashSet<i64> = seeds.iter().copied().collect();
    let related: Vec<String> = ranked.iter().filter(|(n, _)| !seed_set.contains(n)).take(6).map(|(n, _)| node_name(&c, *n)).collect();
    let mut doc_score: HashMap<i64, f64> = HashMap::new();
    for (n, a) in ranked.iter().take(24) {
        if let Ok(mut st) = c.prepare("SELECT doc FROM g_link WHERE node=?1 ORDER BY doc DESC LIMIT 40") {
            if let Ok(rows) = st.query_map([n], |r| r.get::<_, i64>(0)) {
                for d in rows.flatten() {
                    *doc_score.entry(d).or_default() += a;
                }
            }
        }
    }
    let t0 = now();
    let mut docs: Vec<Recalled> = doc_score
        .into_iter()
        .filter_map(|(d, s)| {
            c.query_row("SELECT ts, kind, text FROM g_doc WHERE id=?1", [d], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))
                .ok()
                .map(|(ts, kind, text)| Recalled { ts, kind, text, score: s * 0.5f64.powf((t0 - ts) as f64 / 86400.0 / 30.0) })
        })
        .collect();
    docs.sort_by(|a, b| b.score.total_cmp(&a.score));
    docs.truncate(k);
    (docs, related)
}

/// Entitas dari beberapa dokumen obrolan terakhir (pembawa konteks percakapan panjang).
pub fn recent_entities(n_docs: usize) -> Vec<String> {
    let Ok(c) = open() else { return vec![] };
    c.prepare("SELECT DISTINCT n.name FROM g_link l JOIN g_node n ON n.id=l.node WHERE l.doc IN (SELECT id FROM g_doc WHERE kind IN ('chat','misi','tanya') ORDER BY id DESC LIMIT ?1) ORDER BY n.weight DESC LIMIT 6")
        .and_then(|mut st| st.query_map([n_docs as i64], |r| r.get::<_, String>(0)).map(|r| r.flatten().collect()))
        .unwrap_or_default()
}

/// Kalimat lanjutan ("yang tadi?", "kalau itu?") diperluas dengan topik terakhir.
pub fn expand_query(q: &str) -> String {
    let l = q.to_lowercase();
    let words = l.split_whitespace().count();
    let pron = ["itu", "tadi", "nya", "tersebut", "yang sama", "lanjut", "lagi", "juga"].iter().any(|p| l.contains(p));
    if words <= 4 || pron && words <= 8 {
        let rec = recent_entities(3);
        if !rec.is_empty() {
            return format!("{q} {}", rec.join(" "));
        }
    }
    q.to_string()
}

/// Peluruhan harian: lemahkan hubungan lama, buang yang sudah sangat lemah dan dokumen lebih dari 1 tahun.
pub fn decay() {
    let Ok(c) = open() else { return };
    let _ = c.execute_batch(
        "UPDATE g_node SET weight = weight*0.97; UPDATE g_edge SET w = w*0.97;
         DELETE FROM g_edge WHERE w < 0.15;
         DELETE FROM g_doc WHERE ts < strftime('%s','now','-365 day');
         DELETE FROM g_link WHERE doc NOT IN (SELECT id FROM g_doc);
         DELETE FROM g_node WHERE weight < 0.3 AND id NOT IN (SELECT node FROM g_link);
         DELETE FROM g_edge WHERE a NOT IN (SELECT id FROM g_node) OR b NOT IN (SELECT id FROM g_node);",
    );
}

pub fn stats() -> String {
    let Ok(c) = open() else { return "graf tidak tersedia".into() };
    let n: i64 = c.query_row("SELECT COUNT(*) FROM g_node", [], |r| r.get(0)).unwrap_or(0);
    let e: i64 = c.query_row("SELECT COUNT(*) FROM g_edge", [], |r| r.get(0)).unwrap_or(0);
    let d: i64 = c.query_row("SELECT COUNT(*) FROM g_doc", [], |r| r.get(0)).unwrap_or(0);
    let top: Vec<String> = c
        .prepare("SELECT name, kind, weight FROM g_node ORDER BY weight DESC LIMIT 10")
        .and_then(|mut st| st.query_map([], |r| Ok(format!("{} ({}, {:.0})", r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, f64>(2)?))).map(|r| r.flatten().collect()))
        .unwrap_or_default();
    format!("🕸 Graf ingatan: {n} simpul · {e} hubungan · {d} dokumen.\nPusat perhatian: {}\nTanya: 'apa yang kita bahas tentang X' · /graf X · /graphify <folder> · /graf lihat", if top.is_empty() { "(masih kosong; ngobrol saja, aku mencatatnya)".into() } else { top.join(", ") })
}

/// `/graf <entitas>`: tetangga terkuat + dokumen yang menyebutnya.
pub fn about(topic: &str) -> String {
    let (docs, related) = recall(topic, 5);
    if docs.is_empty() && related.is_empty() {
        return format!("Belum ada ingatan tentang '{topic}'.");
    }
    let mut s = format!("🕸 Ingatan tentang '{topic}':\n");
    if !related.is_empty() {
        s.push_str(&format!("Terkait: {}\n", related.join(", ")));
    }
    for d in docs {
        let when = chrono::DateTime::from_timestamp(d.ts, 0).map(|t| t.with_timezone(&chrono::Local).format("%d/%m").to_string()).unwrap_or_default();
        s.push_str(&format!("• [{when} {}] {}\n", d.kind, d.text.chars().take(160).collect::<String>().replace('\n', " ")));
    }
    s
}

// ---------- Graphify folder (kode & dokumen) ----------

const SKIP: &[&str] = &[".git", "node_modules", "target", ".venv", "venv", "__pycache__", "dist", "build", ".next", ".idea", ".vscode", "bin", "obj"];

fn defs_of(ext: &str, text: &str) -> (Vec<String>, Vec<String>) {
    let (def_re, imp_re): (&str, &str) = match ext {
        "rs" => (r"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:fn|struct|enum|trait|mod)\s+(\w+)", r"(?m)^\s*use\s+([\w:]+)"),
        "py" => (r"(?m)^\s*(?:async\s+)?(?:def|class)\s+(\w+)", r"(?m)^\s*(?:from|import)\s+([\w\.]+)"),
        "js" | "ts" | "tsx" | "jsx" => (r"(?m)(?:function\s+(\w+)|class\s+(\w+)|const\s+(\w+)\s*=\s*(?:async\s*)?\()", r#"(?m)from\s+['"]([^'"]+)['"]"#),
        "go" => (r"(?m)^\s*func\s+(?:\([^)]*\)\s*)?(\w+)", r#"(?m)^\s*"([\w\./\-]+)"$"#),
        "java" | "cs" => (r"(?m)(?:class|interface|enum)\s+(\w+)", r"(?m)^\s*(?:import|using)\s+([\w\.]+)"),
        "c" | "cpp" | "h" | "hpp" => (r"(?m)^\s*(?:[\w\*]+\s+)+\**(\w+)\s*\([^;]*\)\s*\{", r#"(?m)^\s*#include\s+[<"]([^>"]+)[>"]"#),
        _ => return (vec![], vec![]),
    };
    let grab = |re: &str| -> Vec<String> {
        Regex::new(re).map(|r| r.captures_iter(text).filter_map(|c| (1..c.len()).find_map(|i| c.get(i)).map(|m| m.as_str().to_string())).take(40).collect()).unwrap_or_default()
    };
    (grab(def_re), grab(imp_re))
}

/// Nama umum yang tidak informatif sebagai simpul kode.
fn boring(n: &str) -> bool {
    ["tests", "test", "main", "new", "default", "run", "open", "init", "mod", "self", "super", "crate"].contains(&n) || n.starts_with("std::") || n.starts_with("core::") || n.starts_with("super") || n.starts_with("crate")
}

/// Bangun graf dari folder: berkas ↔ fungsi/kelas ↔ impor (+ kata kunci dokumen). Anggaran 8 detik / 1500 berkas.
pub fn graphify_folder(root: &Path) -> String {
    let Ok(mut c) = open() else { return "graf tidak tersedia".into() };
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut files = vec![];
    fn walk(d: &Path, depth: u8, out: &mut Vec<std::path::PathBuf>, deadline: Instant) {
        if depth > 5 || out.len() >= 1500 || Instant::now() > deadline {
            return;
        }
        if let Ok(rd) = std::fs::read_dir(d) {
            for e in rd.flatten() {
                let p = e.path();
                let name = e.file_name().to_string_lossy().to_string();
                if p.is_dir() {
                    if !SKIP.contains(&name.as_str()) && !name.starts_with('.') {
                        walk(&p, depth + 1, out, deadline);
                    }
                } else if p.extension().and_then(|x| x.to_str()).is_some_and(|x| ["rs", "py", "js", "ts", "tsx", "jsx", "go", "java", "cs", "c", "cpp", "h", "hpp", "md", "txt"].contains(&x)) {
                    out.push(p);
                }
            }
        }
    }
    walk(root, 0, &mut files, deadline);
    let ts = now();
    let Ok(tx) = c.transaction() else { return "gagal membuka transaksi".into() };
    let (mut nfiles, mut ndefs, mut nimps) = (0, 0, 0);
    for p in &files {
        if Instant::now() > deadline {
            break;
        }
        let Ok(meta) = std::fs::metadata(p) else { continue };
        if meta.len() > 300_000 {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(p) else { continue };
        let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("").to_string();
        let rel = p.strip_prefix(root).unwrap_or(p).to_string_lossy().replace('\\', "/");
        let fname = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        let Some(fid) = upsert_node(&tx, &Ent { name: fname, kind: "berkas" }, ts, 2.0) else { continue };
        nfiles += 1;
        let (defs, imps) = if ext == "md" || ext == "txt" { (vec![], vec![]) } else { defs_of(&ext, &text) };
        let mut ids = vec![fid];
        for d in defs.iter().filter(|d| !boring(&d.to_lowercase())).take(24) {
            if let Some(id) = upsert_node(&tx, &Ent { name: d.to_lowercase(), kind: "kode" }, ts, 1.0) {
                upsert_edge(&tx, fid, id, 2.0, ts);
                ids.push(id);
                ndefs += 1;
            }
        }
        for i in imps.iter().filter(|i| !boring(&i.to_lowercase())).take(20) {
            if let Some(id) = upsert_node(&tx, &Ent { name: i.to_lowercase(), kind: "kode" }, ts, 0.5) {
                upsert_edge(&tx, fid, id, 1.0, ts);
                nimps += 1;
            }
        }
        if ext == "md" || ext == "txt" {
            for e in extract(&text, 8) {
                if let Some(id) = upsert_node(&tx, &e, ts, 0.5) {
                    upsert_edge(&tx, fid, id, 1.0, ts);
                    ids.push(id);
                }
            }
        }
        for i in 1..ids.len().min(10) {
            for j in i + 1..ids.len().min(10) {
                upsert_edge(&tx, ids[i], ids[j], 0.3, ts);
            }
        }
        let summary = format!("{rel}: {}{}", defs.iter().take(8).cloned().collect::<Vec<_>>().join(", "), if imps.is_empty() { String::new() } else { format!(" | impor: {}", imps.iter().take(4).cloned().collect::<Vec<_>>().join(", ")) });
        if tx.execute("INSERT INTO g_doc(kind,ref,text,ts) VALUES('berkas',?1,?2,?3)", (&rel, &summary, ts)).is_ok() {
            let doc = tx.last_insert_rowid();
            for id in &ids {
                let _ = tx.execute("INSERT OR IGNORE INTO g_link VALUES(?1,?2)", (id, doc));
            }
        }
    }
    let _ = tx.commit();
    format!("🕸 Graphify '{}': {nfiles} berkas, {ndefs} fungsi/kelas, {nimps} impor dimasukkan ke graf. Tanya: 'apa yang kita bahas tentang <nama fungsi>'", root.display())
}

// ---------- Peta graf interaktif (satu berkas HTML, opsional) ----------

pub fn export_html() -> Option<std::path::PathBuf> {
    let c = open().ok()?;
    let nodes: Vec<(i64, String, String, f64)> = c
        .prepare("SELECT id, name, kind, weight FROM g_node ORDER BY weight DESC LIMIT 160")
        .and_then(|mut st| st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).map(|r| r.flatten().collect()))
        .ok()?;
    let ids: HashSet<i64> = nodes.iter().map(|n| n.0).collect();
    let edges: Vec<(i64, i64, f64)> = c
        .prepare("SELECT a, b, w FROM g_edge ORDER BY w DESC LIMIT 600")
        .and_then(|mut st| st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).map(|r| r.flatten().filter(|e: &(i64, i64, f64)| ids.contains(&e.0) && ids.contains(&e.1)).collect()))
        .ok()?;
    let data = serde_json::json!({ "nodes": nodes.iter().map(|n| serde_json::json!({"id": n.0, "name": n.1, "kind": n.2, "w": n.3})).collect::<Vec<_>>(), "edges": edges.iter().map(|e| serde_json::json!([e.0, e.1, e.2])).collect::<Vec<_>>() });
    let html = HTML.replace("__DATA__", &data.to_string().replace("</", "<\\/"));
    let p = crate::config::data_dir().join("graf.html");
    std::fs::write(&p, html).ok()?;
    Some(p)
}

const HTML: &str = r##"<!doctype html><meta charset="utf-8"><title>Graf ingatan NOVA</title>
<style>html,body{margin:0;background:#07101c;color:#cfe9ff;font:13px system-ui}canvas{display:block}#t{position:fixed;left:10px;top:8px;opacity:.85}</style>
<div id="t">Graf ingatan NOVA — seret simpul; arahkan kursor untuk nama. Warna: kata, nama, berkas, domain, kode.</div><canvas id="c"></canvas>
<script>
const D=__DATA__,cv=document.getElementById('c'),g=cv.getContext('2d');let W,H;function rs(){W=cv.width=innerWidth;H=cv.height=innerHeight}rs();onresize=rs;
const col={kata:'#6cc8ff',nama:'#ffd36b',berkas:'#7dffa8',domain:'#ff9bd0',kode:'#c8a2ff'};
const N=D.nodes.map((n,i)=>({...n,x:W/2+Math.cos(i)*200*Math.random(),y:H/2+Math.sin(i)*200*Math.random(),vx:0,vy:0,r:4+Math.min(14,Math.sqrt(n.w)*2)}));
const idx=new Map(N.map(n=>[n.id,n]));const E=D.edges.map(e=>[idx.get(e[0]),idx.get(e[1]),e[2]]).filter(e=>e[0]&&e[1]);
let drag=null,hover=null;
function step(){for(const a of N){for(const b of N){if(a===b)continue;let dx=a.x-b.x,dy=a.y-b.y,d=dx*dx+dy*dy+.1;if(d<90000){const f=900/d;a.vx+=dx*f;a.vy+=dy*f}}a.vx+=(W/2-a.x)*.002;a.vy+=(H/2-a.y)*.002}
for(const[a,b,w]of E){const dx=b.x-a.x,dy=b.y-a.y,d=Math.sqrt(dx*dx+dy*dy)+.1,f=(d-70)*.012*Math.min(2,Math.sqrt(w));a.vx+=dx/d*f;a.vy+=dy/d*f;b.vx-=dx/d*f;b.vy-=dy/d*f}
for(const n of N){if(n===drag)continue;n.vx*=.82;n.vy*=.82;n.x+=n.vx;n.y+=n.vy}}
function draw(){g.clearRect(0,0,W,H);g.lineWidth=1;for(const[a,b,w]of E){g.strokeStyle='rgba(120,180,255,'+Math.min(.5,.08+w*.05)+')';g.beginPath();g.moveTo(a.x,a.y);g.lineTo(b.x,b.y);g.stroke()}
for(const n of N){g.fillStyle=col[n.kind]||'#fff';g.beginPath();g.arc(n.x,n.y,n.r,0,7);g.fill();if(n.r>8||n===hover){g.fillStyle='#fff';g.fillText(n.name,n.x+n.r+3,n.y+4)}}}
let it=0;(function loop(){if(it++<400||drag)step();draw();requestAnimationFrame(loop)})();
cv.onmousemove=e=>{hover=N.find(n=>Math.hypot(n.x-e.x,n.y-e.y)<n.r+3)||null;if(drag){drag.x=e.x;drag.y=e.y}};cv.onmousedown=e=>{drag=hover};onmouseup=()=>drag=null;
</script>"##;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extracts_entities_by_kind() {
        let e = extract("Tadi Budi memperbaiki fungsi parse_header di main.py untuk server api.example.com dengan UserCard", 12);
        let names: Vec<(&str, &str)> = e.iter().map(|x| (x.name.as_str(), x.kind)).collect();
        assert!(names.contains(&("main.py", "berkas")), "{names:?}");
        assert!(names.contains(&("api.example.com", "domain")));
        assert!(names.contains(&("parse_header", "kode")));
        assert!(names.contains(&("usercard", "kode")));
        assert!(names.contains(&("memperbaiki", "kata")));
        assert!(!names.iter().any(|n| n.0 == "untuk"));
    }
    #[test]
    fn graph_remembers_and_recalls_across_documents() {
        // entitas unik agar tidak bertabrakan dengan data nyata di basis pengetahuan
        ingest("chat", "Proyek zxqorbit memakai modul zxqbaterai untuk prediksi daya laptop");
        ingest("chat", "Besok zxqbaterai perlu kalibrasi sensor zxqtegangan");
        ingest("chat", "Resep nasi goreng zxqkuliner pakai bawang");
        let (docs, related) = recall("apa kabar zxqbaterai?", 3);
        assert!(!docs.is_empty());
        assert!(docs.iter().any(|d| d.text.contains("zxqbaterai")));
        assert!(!docs.iter().any(|d| d.text.contains("zxqkuliner")));
        assert!(related.iter().any(|r| r.contains("zxqorbit") || r.contains("zxqtegangan") || r == "modul" || r == "laptop"), "{related:?}");
    }
    #[test]
    fn defs_extraction() {
        let (d, i) = defs_of("py", "import os\nfrom a.b import c\n\ndef hitung(x):\n    pass\nclass Kalkulator:\n    pass\n");
        assert!(d.contains(&"hitung".to_string()) && d.contains(&"Kalkulator".to_string()));
        assert!(i.contains(&"os".to_string()) && i.contains(&"a.b".to_string()));
    }
    #[test]
    fn follow_up_gets_expanded() {
        ingest("chat", "Kita membahas zxqkalibrasi sensor tegangan inverter");
        let q = expand_query("kalau yang itu?");
        assert!(q.contains("zxq"), "{q}");
    }
}

/// (simpul, sisi)
pub fn hitung() -> (i64, i64) {
    let Ok(c) = open() else { return (0, 0) };
    (c.query_row("SELECT COUNT(*) FROM g_node", [], |r| r.get(0)).unwrap_or(0), c.query_row("SELECT COUNT(*) FROM g_edge", [], |r| r.get(0)).unwrap_or(0))
}

/// Simpul teratas (menurut bobot) beserta sisi di antara mereka, untuk tampilan graf di dasbor.
pub fn json(limit: usize) -> serde_json::Value {
    let Ok(c) = open() else { return serde_json::json!({"nodes": [], "edges": []}) };
    let nodes: Vec<(i64, String, String, f64)> = c
        .prepare("SELECT id, name, kind, weight FROM g_node WHERE length(name) > 3 AND name NOT IN ('buka','coba','carikan','cari','tolong','bisa','lalu','terus','dong','saja','yang','dengan','untuk','tentang','minta','tampilkan','lihat','zxqbaterai') ORDER BY weight DESC LIMIT ?1")
        .and_then(|mut st| st.query_map([limit as i64], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).map(|r| r.flatten().collect()))
        .unwrap_or_default();
    let ids: std::collections::HashSet<i64> = nodes.iter().map(|n| n.0).collect();
    let edges: Vec<(i64, i64, f64)> = c
        .prepare("SELECT a, b, w FROM g_edge ORDER BY w DESC LIMIT 6000")
        .and_then(|mut st| st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).map(|r| r.flatten().filter(|e: &(i64, i64, f64)| ids.contains(&e.0) && ids.contains(&e.1)).collect()))
        .unwrap_or_default();
    serde_json::json!({
        "nodes": nodes.iter().map(|n| serde_json::json!({"id": n.0, "name": n.1, "kind": n.2, "w": n.3})).collect::<Vec<_>>(),
        "edges": edges.iter().map(|e| serde_json::json!([e.0, e.1, e.2])).collect::<Vec<_>>(),
    })
}
