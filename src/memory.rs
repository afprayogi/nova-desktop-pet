use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::Mutex;

type Row3 = (i64, String, String);

/// Memori bertingkat di SQLite:
/// messages (jangka pendek/episodik), summary (ringkasan berjalan),
/// notes (semantik, per agen atau 'shared'), decrees (titah Bos, prioritas tertinggi).
pub struct Memory {
    conn: Mutex<Connection>,
}

const DIM: usize = 256;

fn fnv(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Embedding lokal (feature hashing unigram+bigram). Tanpa API, cukup untuk retrieval sederhana.
pub fn embed(text: &str) -> Vec<f32> {
    let mut v = vec![0f32; DIM];
    let lower = text.to_lowercase();
    let toks: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.chars().count() > 1)
        .collect();
    let mut add = |s: &str, w: f32| {
        let h = fnv(s);
        let sign = if (h >> 40) & 1 == 0 { 1.0 } else { -1.0 };
        v[(h as usize) % DIM] += sign * w;
    };
    for (i, t) in toks.iter().enumerate() {
        add(t, 1.0);
        if i + 1 < toks.len() {
            add(&format!("{} {}", t, toks[i + 1]), 0.7);
        }
    }
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 0.0 {
        v.iter_mut().for_each(|x| *x /= n);
    }
    v
}

fn to_bytes(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

fn from_bytes(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

impl Memory {
    pub fn open(path: &Path) -> Result<Self> {
        let c = Connection::open(path)?;
        c.execute_batch(
            "CREATE TABLE IF NOT EXISTS messages(id INTEGER PRIMARY KEY, ts INTEGER DEFAULT (strftime('%s','now')), speaker TEXT, content TEXT);
             CREATE TABLE IF NOT EXISTS notes(id INTEGER PRIMARY KEY, ts INTEGER DEFAULT (strftime('%s','now')), owner TEXT, kind TEXT, content TEXT, source TEXT, emb BLOB);
             CREATE TABLE IF NOT EXISTS summary(id INTEGER PRIMARY KEY, upto INTEGER, text TEXT);
             CREATE TABLE IF NOT EXISTS decrees(id INTEGER PRIMARY KEY, ts INTEGER DEFAULT (strftime('%s','now')), text TEXT, active INTEGER DEFAULT 1);
             CREATE TABLE IF NOT EXISTS model_stats(id TEXT PRIMARY KEY, ok INTEGER, fail INTEGER, vgood INTEGER, vbad INTEGER, avg_ms REAL);",
        )?;
        Ok(Self {
            conn: Mutex::new(c),
        })
    }

    pub fn add_message(&self, speaker: &str, content: &str) -> Result<i64> {
        let c = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        c.execute(
            "INSERT INTO messages(speaker,content) VALUES(?1,?2)",
            params![speaker, content],
        )?;
        Ok(c.last_insert_rowid())
    }

    pub fn latest_message_id(&self) -> Result<i64> {
        let c = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        Ok(
            c.query_row("SELECT COALESCE(MAX(id), 0) FROM messages", [], |row| {
                row.get(0)
            })?,
        )
    }

    pub fn recent_messages_after(&self, after: i64, n: usize) -> Result<Vec<Row3>> {
        let c = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut st = c.prepare(
            "SELECT id,speaker,content FROM messages WHERE id>?1 ORDER BY id DESC LIMIT ?2",
        )?;
        let rows = st.query_map(params![after, n as i64], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?;
        let mut v = rows.collect::<std::result::Result<Vec<Row3>, _>>()?;
        v.reverse();
        Ok(v)
    }

    pub fn messages_after(&self, id: i64) -> Result<Vec<Row3>> {
        let c = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut st =
            c.prepare("SELECT id,speaker,content FROM messages WHERE id>?1 ORDER BY id")?;
        let rows = st.query_map(params![id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        Ok(rows.collect::<std::result::Result<Vec<Row3>, _>>()?)
    }

    pub fn add_note(&self, owner: &str, kind: &str, content: &str, source: &str) -> Result<()> {
        let emb = to_bytes(&embed(content));
        let c = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        c.execute(
            "INSERT INTO notes(owner,kind,content,source,emb) VALUES(?1,?2,?3,?4,?5)",
            params![owner, kind, content, source, emb],
        )?;
        Ok(())
    }

    /// Ambil catatan paling relevan milik agen + papan bersama.
    pub fn search_notes(&self, owner: &str, query: &str, k: usize) -> Result<Vec<String>> {
        let q = embed(query);
        let c = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut st = c.prepare(
            "SELECT content,emb FROM notes WHERE owner='shared' OR owner=?1 ORDER BY id DESC LIMIT 400",
        )?;
        let rows = st.query_map(params![owner], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
        })?;
        let mut scored: Vec<(f32, String)> = vec![];
        for r in rows {
            let (content, emb) = r?;
            let e = from_bytes(&emb);
            let sim: f32 = q.iter().zip(e.iter()).map(|(a, b)| a * b).sum();
            let topic_terms = |text: &str| {
                text.to_lowercase()
                    .split(|c: char| !c.is_alphanumeric())
                    .filter(|term| {
                        term.chars().count() >= 4
                            && !matches!(
                                *term,
                                "yang"
                                    | "dari"
                                    | "untuk"
                                    | "dengan"
                                    | "pada"
                                    | "dalam"
                                    | "adalah"
                                    | "atau"
                                    | "juga"
                                    | "tentang"
                                    | "bagaimana"
                                    | "mengapa"
                            )
                    })
                    .map(str::to_string)
                    .collect::<std::collections::HashSet<_>>()
            };
            let query_terms = topic_terms(query);
            let note_terms = topic_terms(&content);
            let has_shared_term = query_terms.iter().any(|term| note_terms.contains(term));
            if sim > 0.18 && has_shared_term {
                scored.push((sim, content));
            }
        }
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        Ok(scored.into_iter().take(k).map(|x| x.1).collect())
    }

    pub fn get_summary(&self) -> Result<(i64, String)> {
        let c = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let r = c
            .query_row("SELECT upto,text FROM summary WHERE id=1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()?;
        Ok(r.unwrap_or((0, String::new())))
    }

    pub fn set_summary(&self, upto: i64, text: &str) -> Result<()> {
        let c = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        c.execute(
            "INSERT OR REPLACE INTO summary(id,upto,text) VALUES(1,?1,?2)",
            params![upto, text],
        )?;
        Ok(())
    }

    pub fn add_decree(&self, text: &str) -> Result<i64> {
        let c = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        c.execute("INSERT INTO decrees(text) VALUES(?1)", params![text])?;
        Ok(c.last_insert_rowid())
    }

    pub fn remove_decree(&self, id: i64) -> Result<()> {
        let c = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        c.execute("UPDATE decrees SET active=0 WHERE id=?1", params![id])?;
        Ok(())
    }

    pub fn decrees(&self) -> Result<Vec<(i64, String)>> {
        let c = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut st = c.prepare("SELECT id,text FROM decrees WHERE active=1 ORDER BY id")?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<std::result::Result<Vec<(i64, String)>, _>>()?)
    }

    pub fn load_stats(&self) -> Vec<(String, u32, u32, u32, u32, f64)> {
        let c = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let Ok(mut st) = c.prepare("SELECT id,ok,fail,vgood,vbad,avg_ms FROM model_stats") else {
            return vec![];
        };
        let Ok(rows) = st.query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
        }) else {
            return vec![];
        };
        rows.filter_map(|r| r.ok()).collect()
    }

    pub fn save_stats(&self, rows: Vec<(String, u32, u32, u32, u32, f64)>) {
        let c = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let _ = c.execute_batch("BEGIN");
        for (id, ok, fail, vg, vb, ms) in rows {
            let _ = c.execute(
                "INSERT OR REPLACE INTO model_stats(id,ok,fail,vgood,vbad,avg_ms) VALUES(?1,?2,?3,?4,?5,?6)",
                params![id, ok, fail, vg, vb, ms],
            );
        }
        let _ = c.execute_batch("COMMIT");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_roundtrip_and_retrieval() {
        let path = std::env::temp_dir().join(format!("kh-test-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let m = Memory::open(&path).unwrap();
        m.add_note("shared", "riset", "Ibu kota Jepang adalah Tokyo", "")
            .unwrap();
        m.add_note(
            "shared",
            "riset",
            "Resep nasi goreng memakai kecap manis",
            "",
        )
        .unwrap();
        let r = m
            .search_notes("Raja Singa", "apa ibu kota Jepang", 1)
            .unwrap();
        assert!(r[0].contains("Tokyo"));
        let id = m.add_decree("Jangan bicara politik").unwrap();
        assert_eq!(m.decrees().unwrap().len(), 1);
        m.remove_decree(id).unwrap();
        assert!(m.decrees().unwrap().is_empty());
        m.add_message("BOS", "halo").unwrap();
        let first_message = m.latest_message_id().unwrap();
        let last_message = m.add_message("Raja Singa", "halo bos").unwrap();
        assert_eq!(m.recent_messages_after(first_message, 10).unwrap().len(), 1);
        m.set_summary(1, "ringkas").unwrap();
        assert_eq!(m.get_summary().unwrap().1, "ringkas");
        assert!(m.messages_after(last_message).unwrap().is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn unrelated_old_notes_are_not_recalled_from_embedding_collisions() {
        let path = std::env::temp_dir().join(format!("kh-relevance-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let memory = Memory::open(&path).unwrap();
        memory
            .add_note(
                "shared",
                "riset",
                "Resep nasi goreng memakai kecap manis",
                "",
            )
            .unwrap();
        assert!(memory
            .search_notes("Raja Singa", "kebijakan pendidikan sekolah", 4)
            .unwrap()
            .is_empty());
        let _ = std::fs::remove_file(&path);
    }
}
