use rand::seq::SliceRandom;
use rand::Rng;
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Clone, serde::Serialize)]
pub struct PoolRow {
    pub id: String,
    pub score: f64,
    pub avg_ms: f64,
    pub ok: u32,
    pub fail: u32,
}

#[derive(Default, Clone)]
pub struct ModelStat {
    pub ok: u32,
    pub fail: u32,
    pub vgood: u32,
    pub vbad: u32,
    pub avg_ms: f64,
    pub consec_fail: u32,
    pub blocked_until: Option<Instant>,
    pub ready: bool,
}

/// Tiga lapis: katalog (besar) -> kolam aktif (ready) -> 5 kursi agen.
#[derive(Default)]
pub struct Router {
    pub catalog: Vec<String>,
    pub stats: HashMap<String, ModelStat>,
}

const SKIP: [&str; 18] = [
    "embed", "whisper", "tts", "dall", "image", "moderation", "rerank", "audio", "transcribe", "sora",
    "diffusion", "flux", "midjourney", "imagen", "veo", "video", "aihorde", "ui-tars",
];

impl Router {
    pub fn load(&mut self, rows: Vec<(String, u32, u32, u32, u32, f64)>) {
        for (id, ok, fail, vgood, vbad, avg_ms) in rows {
            let s = self.stats.entry(id).or_default();
            s.ok = ok;
            s.fail = fail;
            s.vgood = vgood;
            s.vbad = vbad;
            s.avg_ms = avg_ms;
        }
    }

    pub fn export(&self) -> Vec<(String, u32, u32, u32, u32, f64)> {
        self.stats
            .iter()
            .map(|(k, s)| (k.clone(), s.ok, s.fail, s.vgood, s.vbad, s.avg_ms))
            .collect()
    }

    pub fn set_catalog(&mut self, ids: Vec<String>) {
        self.catalog = ids;
    }

    fn is_chat_model(id: &str) -> bool {
        let l = id.to_lowercase();
        !SKIP.iter().any(|k| l.contains(k))
    }

    /// Pilih kandidat uji: yang pernah bagus dulu (cek ulang), sisanya acak (eksplorasi).
    pub fn candidates(&self, limit: usize) -> Vec<String> {
        let mut rng = rand::thread_rng();
        let mut known = vec![];
        let mut fresh = vec![];
        let now = Instant::now();
        for id in self.catalog.iter().filter(|i| Self::is_chat_model(i)) {
            let stat = self.stats.get(id);
            if stat
                .and_then(|s| s.blocked_until)
                .map(|t| t > now)
                .unwrap_or(false)
            {
                continue;
            }
            match stat {
                Some(s) if s.ok > 0 && s.vgood >= s.vbad => known.push(id.clone()),
                _ => fresh.push(id.clone()),
            }
        }
        known.shuffle(&mut rng);
        fresh.shuffle(&mut rng);
        // Model gratis (":free" / "free") diuji lebih dulu; model berbayar hanya mengisi sisa kursi.
        fresh.sort_by_key(|id| !id.to_lowercase().contains("free"));
        let mut out: Vec<String> = known.into_iter().take(limit / 2).collect();
        let room = limit.saturating_sub(out.len());
        out.extend(fresh.into_iter().take(room));
        out
    }

    pub fn score(&self, id: &str) -> f64 {
        let s = self.stats.get(id).cloned().unwrap_or_default();
        let quality = (s.vgood as f64 + 1.0) / (s.vgood as f64 + s.vbad as f64 + 2.0);
        let reliab = (s.ok as f64 + 1.0) / (s.ok as f64 + s.fail as f64 + 2.0);
        let speed = 1.0 / (1.0 + s.avg_ms / 8000.0);
        quality * reliab * speed
    }

    pub fn available(&self, id: &str) -> bool {
        self.stats
            .get(id)
            .map(|s| s.ready && s.blocked_until.map(|t| t <= Instant::now()).unwrap_or(true))
            .unwrap_or(false)
    }

    /// Pilih model ready berskor tinggi dengan sedikit eksplorasi acak (mirip bandit).
    pub fn pick(&self, exclude: &[String]) -> Option<String> {
        let mut rng = rand::thread_rng();
        self.stats
            .keys()
            .filter(|id| self.available(id) && !exclude.contains(id))
            .map(|id| (id.clone(), self.score(id) * rng.gen_range(0.95..1.05)))
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(id, _)| id)
    }

    pub fn mark_ready(&mut self, id: &str, ms: f64) {
        let s = self.stats.entry(id.to_string()).or_default();
        s.ready = true;
        s.blocked_until = None;
        s.consec_fail = 0;
        s.ok += 1;
        s.avg_ms = if s.avg_ms == 0.0 { ms } else { s.avg_ms * 0.7 + ms * 0.3 };
    }

    pub fn mark_down(&mut self, id: &str) {
        let s = self.stats.entry(id.to_string()).or_default();
        s.ready = false;
        s.fail += 1;
    }

    pub fn record_ok(&mut self, id: &str, ms: f64) {
        let s = self.stats.entry(id.to_string()).or_default();
        s.ok += 1;
        s.consec_fail = 0;
        s.avg_ms = if s.avg_ms == 0.0 { ms } else { s.avg_ms * 0.7 + ms * 0.3 };
    }

    /// Circuit breaker: 3 gagal beruntun -> diblokir 5 menit.
    pub fn record_fail(&mut self, id: &str) {
        let s = self.stats.entry(id.to_string()).or_default();
        s.fail += 1;
        s.consec_fail += 1;
        if s.consec_fail >= 3 {
            s.ready = false;
            s.blocked_until = Some(Instant::now() + Duration::from_secs(300));
        }
    }

    pub fn record_verdict(&mut self, id: &str, good: bool) {
        let s = self.stats.entry(id.to_string()).or_default();
        if good {
            s.vgood += 1;
            s.consec_fail = 0;
        } else {
            s.vbad += 1;
            s.consec_fail += 1;
            if s.consec_fail >= 3 {
                s.ready = false;
                s.blocked_until = Some(Instant::now() + Duration::from_secs(300));
            }
        }
    }

    pub fn pool(&self) -> Vec<PoolRow> {
        let mut v: Vec<PoolRow> = self
            .stats
            .iter()
            .filter(|(id, _)| self.available(id))
            .map(|(id, s)| PoolRow {
                id: id.clone(),
                score: self.score(id),
                avg_ms: s.avg_ms,
                ok: s.ok,
                fail: s.fail,
            })
            .collect();
        v.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_around_failures() {
        let mut r = Router::default();
        r.set_catalog(vec!["a".into(), "b".into(), "text-embedding-3".into()]);
        assert!(!r.candidates(10).contains(&"text-embedding-3".to_string()));
        r.mark_ready("a", 500.0);
        r.mark_ready("b", 900.0);
        assert_eq!(r.pool().len(), 2);
        // 'a' gagal 3x beruntun -> diblokir, router hanya memilih 'b'
        for _ in 0..3 {
            r.record_fail("a");
        }
        assert!(!r.available("a"));
        for _ in 0..20 {
            assert_eq!(r.pick(&[]).as_deref(), Some("b"));
        }
        assert_eq!(r.pick(&["b".to_string()]), None);
    }

    #[test]
    fn blocked_known_model_is_not_retested() {
        let mut r = Router::default();
        r.set_catalog(vec!["known-model".into()]);
        r.mark_ready("known-model", 500.0);
        r.record_verdict("known-model", true);
        for _ in 0..3 {
            r.record_fail("known-model");
        }

        assert!(r.candidates(10).is_empty());
    }

    #[test]
    fn good_verdict_resets_consecutive_failures() {
        let mut r = Router::default();
        r.mark_ready("model", 500.0);
        r.record_fail("model");
        r.record_fail("model");
        r.record_verdict("model", true);
        r.record_fail("model");

        assert!(r.available("model"));
    }

    #[test]
    fn score_handles_maximum_counters() {
        let mut r = Router::default();
        r.stats.insert(
            "model".into(),
            ModelStat {
                ok: u32::MAX,
                fail: u32::MAX,
                vgood: u32::MAX,
                vbad: u32::MAX,
                ready: true,
                ..Default::default()
            },
        );

        assert!(r.score("model").is_finite());
    }
}