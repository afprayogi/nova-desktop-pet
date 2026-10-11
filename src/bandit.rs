//! Kepribadian adaptif: contextual bandit (UCB1) yang belajar tindakan proaktif mana yang disukai Bos pada konteks tertentu
//! (mode kerja x bagian hari): kapan diam, kapan menampilkan adegan, kapan cukup notifikasi halus, gaya balasan emosi.
//! Hadiah: respons Bos (1.0), 👍 (1.0), 👎 (0.0 + jeda 30 menit), diabaikan (0.2; untuk "skip" 0.5). Data kecil di assistant.db.
use std::sync::Mutex;
use std::time::{Duration, Instant};

struct Pending {
    ctx: String,
    action: String,
    at: Instant,
}

static PENDING: Mutex<Vec<Pending>> = Mutex::new(Vec::new());
static QUIET_UNTIL: Mutex<Option<Instant>> = Mutex::new(None);

pub fn ensure(c: &rusqlite::Connection) {
    let _ = c.execute_batch("CREATE TABLE IF NOT EXISTS bandit(ctx TEXT, action TEXT, n INT, sum REAL, PRIMARY KEY(ctx,action));");
}

/// Konteks: jenis x mode kerja x bagian hari (blok 4 jam).
pub fn ctx(kind: &str) -> String {
    let h = crate::assistant::day_min() / 60;
    format!("{kind}|{}|{}", crate::konteks::current().0.label(), h / 4)
}

fn stats(c: &rusqlite::Connection, ctx: &str, a: &str) -> (f64, f64) {
    c.query_row("SELECT n, sum FROM bandit WHERE ctx=?1 AND action=?2", (ctx, a), |r| Ok((r.get::<_, i64>(0)? as f64, r.get::<_, f64>(1)?))).unwrap_or((0.0, 0.0))
}

/// Pilih tindakan dengan UCB1 (yang belum pernah dicoba didahulukan, acak di antaranya).
pub fn choose(ctx: &str, actions: &[&str]) -> String {
    if actions.is_empty() {
        return "diam".into();
    }
    let Ok(c) = crate::assistant::db() else { return actions[0].to_string() };
    ensure(&c);
    let st: Vec<(f64, f64)> = actions.iter().map(|a| stats(&c, ctx, a)).collect();
    let untried: Vec<usize> = (0..actions.len()).filter(|i| st[*i].0 == 0.0).collect();
    if !untried.is_empty() {
        let pick = untried[rand::random::<usize>() % untried.len()];
        return actions[pick].to_string();
    }
    let total: f64 = st.iter().map(|s| s.0).sum();
    let best = (0..actions.len())
        .max_by(|a, b| ucb(st[*a], total).total_cmp(&ucb(st[*b], total)))
        .unwrap_or(0);
    actions[best].to_string()
}

fn ucb(s: (f64, f64), total: f64) -> f64 {
    s.1 / s.0 + (2.0 * total.max(1.0).ln() / s.0).sqrt()
}

pub fn reward(ctx: &str, action: &str, r: f64) {
    if let Ok(c) = crate::assistant::db() {
        ensure(&c);
        let _ = c.execute("INSERT INTO bandit VALUES(?1,?2,1,?3) ON CONFLICT(ctx,action) DO UPDATE SET n=n+1, sum=sum+?3", (ctx, action, r));
    }
}

/// Catat tindakan proaktif yang menunggu reaksi Bos.
pub fn expect(ctx: &str, action: &str) {
    if let Ok(mut p) = PENDING.lock() {
        p.push(Pending { ctx: ctx.to_string(), action: action.to_string(), at: Instant::now() });
        let n = p.len();
        if n > 20 {
            p.drain(0..n - 20);
        }
    }
}

/// Bos membalas/berinteraksi: hadiah penuh untuk tindakan dalam 2 menit terakhir.
pub fn engaged() {
    let done: Vec<Pending> = match PENDING.lock() {
        Ok(mut p) => {
            let (hit, keep): (Vec<_>, Vec<_>) = p.drain(..).partition(|x| x.at.elapsed() < Duration::from_secs(120));
            *p = keep;
            hit
        }
        Err(_) => return,
    };
    for x in done {
        reward(&x.ctx, &x.action, if x.action == "skip" { 0.5 } else { 1.0 });
    }
}

/// 👍/👎 untuk tindakan proaktif terakhir.
pub fn feedback(good: bool) -> bool {
    let last = PENDING.lock().ok().and_then(|mut p| p.pop());
    let Some(x) = last else { return false };
    reward(&x.ctx, &x.action, if good { 1.0 } else { 0.0 });
    if !good {
        if let Ok(mut q) = QUIET_UNTIL.lock() {
            *q = Some(Instant::now() + Duration::from_secs(1800));
        }
    }
    true
}

pub fn quiet() -> bool {
    QUIET_UNTIL.lock().map(|q| q.is_some_and(|t| Instant::now() < t)).unwrap_or(false)
}

pub fn set_quiet(minutes: u64) {
    if let Ok(mut q) = QUIET_UNTIL.lock() {
        *q = Some(Instant::now() + Duration::from_secs(minutes * 60));
    }
}

/// Tindakan yang tidak direspons selama 5 menit: hadiah kecil (dipanggil berkala).
pub fn expire_stale() {
    let stale: Vec<Pending> = match PENDING.lock() {
        Ok(mut p) => {
            let (old, keep): (Vec<_>, Vec<_>) = p.drain(..).partition(|x| x.at.elapsed() > Duration::from_secs(300));
            *p = keep;
            old
        }
        Err(_) => return,
    };
    for x in stale {
        reward(&x.ctx, &x.action, if x.action == "skip" { 0.5 } else { 0.2 });
    }
}

/// Ringkasan "kepribadian" yang sudah dipelajari.
pub fn report() -> String {
    let Ok(c) = crate::assistant::db() else { return "belum ada data".into() };
    ensure(&c);
    let rows: Vec<(String, String, i64, f64)> = c
        .prepare("SELECT ctx, action, n, sum FROM bandit WHERE n >= 2 ORDER BY (sum/n) DESC, n DESC LIMIT 8")
        .and_then(|mut st| st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).map(|r| r.flatten().collect()))
        .unwrap_or_default();
    if rows.is_empty() {
        return "Aku masih belajar apa yang Bos suka (kapan diam, kapan menghibur, kapan mengingatkan). Beri 👍/👎 atau cukup balas saat aku menyapa.".into();
    }
    let mut s = String::from("Yang kupelajari tentang preferensi Bos (konteks → tindakan, skor 0-1):");
    for (cx, a, n, sum) in rows {
        s.push_str(&format!("\n• {cx} → {a}: {:.2} ({n}x)", sum / n as f64));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ucb_prefers_rewarded_action() {
        let total = 20.0;
        assert!(ucb((10.0, 9.0), total) > ucb((10.0, 2.0), total));
        // yang jarang dicoba mendapat bonus eksplorasi
        assert!(ucb((1.0, 0.5), total) > ucb((10.0, 5.0), total));
    }
}
