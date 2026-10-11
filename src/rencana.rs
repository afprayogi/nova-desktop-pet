//! Perencana tidur & jadwal: menggabungkan jadwal tetap (kerja, explore), pengingat, dan acara kalender (ICS/Google/Outlook) untuk
//! menjawab "apakah malam ini saya masih bisa tidur 8 jam?". Murni perhitungan lokal, tanpa model.
use crate::assistant;
use chrono::{DateTime, Datelike, Duration, Local, NaiveTime, TimeZone, Timelike};

const PREP_MIN: i64 = 45; // bangun -> siap mulai
const WIND_MIN: i64 = 30; // selesai aktivitas -> berbaring
const MIN_SLEEP_GAP_H: i64 = 3; // komitmen yang lebih dekat dari ini dianggap masih bagian malam yang sama

struct Item {
    start: DateTime<Local>,
    end: DateTime<Local>,
    label: String,
}

fn at(day: chrono::NaiveDate, hm: &str) -> Option<DateTime<Local>> {
    let (h, m) = hm.split_once(':').unwrap_or((hm, "0"));
    let (h, m): (u32, u32) = (h.trim().parse().ok()?, m.trim().parse().ok()?);
    if h >= 24 {
        return Local.from_local_datetime(&(day.succ_opt()?.and_time(NaiveTime::from_hms_opt(h - 24, m, 0)?))).single();
    }
    Local.from_local_datetime(&day.and_time(NaiveTime::from_hms_opt(h, m, 0)?)).single()
}

fn hm(d: DateTime<Local>) -> String {
    d.format("%H:%M").to_string()
}

fn dur(mins: i64) -> String {
    let m = mins.max(0);
    format!("{}j {:02}m", m / 60, m % 60)
}

fn day_name(d: DateTime<Local>, now: DateTime<Local>) -> String {
    let n = (d.date_naive() - now.date_naive()).num_days();
    match n {
        0 => "hari ini".into(),
        1 => "besok".into(),
        _ => ["Senin", "Selasa", "Rabu", "Kamis", "Jumat", "Sabtu", "Minggu"][d.weekday().num_days_from_monday() as usize].into(),
    }
}

fn commitments(now: DateTime<Local>) -> (Vec<Item>, Option<Item>) {
    let s = assistant::schedule();
    let mut v: Vec<Item> = vec![];
    let mut explore = None;
    for off in 0..3 {
        let day = (now + Duration::days(off)).date_naive();
        for (a, b) in &s.work {
            if let (Some(st), Some(en)) = (at(day, a), at(day, b)) {
                v.push(Item { start: st, end: en, label: format!("Jam kerja {a}-{b}") });
            }
        }
        if let (Some(st), Some(en)) = (at(day, &s.explore.0), at(day, &s.explore.1)) {
            if off == 0 || en > now {
                let it = Item { start: st, end: en, label: format!("Explore {}-{}", s.explore.0, s.explore.1) };
                if explore.is_none() && en > now {
                    explore = Some(Item { start: it.start, end: it.end, label: it.label.clone() });
                }
            }
        }
    }
    if let Ok(c) = assistant::db() {
        let from = now.timestamp();
        let to = (now + Duration::days(2)).timestamp();
        if let Ok(mut st) = c.prepare("SELECT due, text FROM rem WHERE done=0 AND due BETWEEN ?1 AND ?2 ORDER BY due") {
            if let Ok(rows) = st.query_map([from, to], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))) {
                for (due, t) in rows.flatten() {
                    if let Some(d) = Local.timestamp_opt(due, 0).single() {
                        v.push(Item { start: d, end: d + Duration::minutes(15), label: format!("Pengingat: {}", t.chars().take(40).collect::<String>()) });
                    }
                }
            }
        }
    }
    for e in crate::konteks::events() {
        if e.start >= now && e.start <= now + Duration::days(2) {
            v.push(Item { start: e.start, end: e.start + Duration::minutes(60), label: format!("Acara: {}", e.title.chars().take(40).collect::<String>()) });
        }
    }
    v.sort_by_key(|i| i.start);
    (v, explore)
}

/// Laporan lengkap: bisakah tidur `target` jam sebelum komitmen berikutnya?
pub fn plan() -> String {
    let now = Local::now();
    let s = assistant::schedule();
    let target_min = (s.sleep_hours * 60.0) as i64;
    let (items, explore) = commitments(now);
    // Lewat tengah malam (sebelum 06:00) explore malam tadi sudah lewat; yang dihitung adalah sisa malam ini.
    let explore = if now.hour() < 6 { None } else { explore };
    let cap = at(now.date_naive(), &s.explore_cap).map(|d| if d < now - Duration::hours(12) { d + Duration::days(1) } else { d });

    // Akhir aktivitas malam ini: sesuai jadwal (explore sampai selesai) atau, bila tidak ada explore, komitmen malam terakhir.
    let evening_end = |use_cap: bool| -> DateTime<Local> {
        let mut end = now;
        if let Some(ex) = &explore {
            if ex.end > now {
                end = if use_cap { cap.filter(|c| *c > now && *c < ex.end).unwrap_or(ex.end) } else { ex.end };
            }
        }
        // komitmen malam (pengingat/acara) yang jatuh sebelum komitmen kerja pagi berikutnya
        for it in items.iter().filter(|i| !i.label.starts_with("Jam kerja")) {
            if it.start < now + Duration::hours(14) && it.end > end && it.start.hour_ok(now) {
                end = it.end;
            }
        }
        end
    };

    let next_commit = |after: DateTime<Local>| -> Option<&Item> { items.iter().filter(|i| i.start >= after + Duration::hours(MIN_SLEEP_GAP_H)).min_by_key(|i| i.start) };

    let mut out = format!("🛌 Rencana tidur (target {} jam)\n", s.sleep_hours);
    let sched_end = evening_end(false);
    let cap_end = evening_end(true);
    let Some(first) = next_commit(sched_end.min(cap_end)) else {
        return "Tidak ada komitmen terjadwal setelah malam ini; tidurmu bebas. 🛏".into();
    };
    let wake_by = first.start - Duration::minutes(PREP_MIN);
    out.push_str(&format!("Komitmen pertama berikutnya: {} {} ({})\nBangun paling lambat: {} (siap {PREP_MIN} mnt)\n", day_name(first.start, now), hm(first.start), first.label, hm(wake_by)));

    let scenario = |label: &str, end: DateTime<Local>| -> (String, bool) {
        let lie = end + Duration::minutes(WIND_MIN);
        let slept = (wake_by - lie).num_minutes();
        let ok = slept >= target_min;
        (format!("{}: selesai {} → berbaring {} → tidur {} ({})", label, hm(end), hm(lie), dur(slept), if ok { "✅ cukup".to_string() } else { format!("⚠ kurang {}", dur(target_min - slept)) }), ok)
    };
    let (l1, ok1) = scenario("Sesuai jadwal", sched_end);
    out.push_str(&format!("{l1}\n"));
    if cap_end != sched_end {
        let (l2, _) = scenario(&format!("Explore ditutup {}", hm(cap_end)), cap_end);
        out.push_str(&format!("{l2}\n"));
    }

    let latest_end = wake_by - Duration::minutes(target_min + WIND_MIN);
    if ok1 {
        out.push_str(&format!("✅ Kamu masih bisa tidur {} jam. Batas selesai aktivitas: {}.", s.sleep_hours, hm(latest_end)));
    } else {
        out.push_str(&format!("➡ Agar tidur {} jam, selesaikan semua aktivitas paling lambat {} (sekarang {}).", s.sleep_hours, hm(latest_end), hm(now)));
        let late: Vec<String> = items.iter().filter(|i| i.start > latest_end && i.start < first.start - Duration::hours(1)).map(|i| format!("{} {}", hm(i.start), i.label)).take(3).collect();
        if !late.is_empty() {
            out.push_str(&format!("\nYang melewati batas itu: {}.", late.join("; ")));
        }
        if let Some(ex) = &explore {
            if ex.end > latest_end && ex.start < latest_end {
                out.push_str(&format!("\nSaran: pendekkan explore {} menit (tutup {}).", (ex.end - latest_end).num_minutes().max(0), hm(latest_end)));
            }
        }
    }
    out
}

trait HourOk {
    fn hour_ok(&self, now: DateTime<Local>) -> bool;
}

impl HourOk for DateTime<Local> {
    /// Hanya komitmen sampai ±14 jam ke depan yang dianggap aktivitas malam ini.
    fn hour_ok(&self, now: DateTime<Local>) -> bool {
        *self >= now && *self <= now + Duration::hours(14)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plan_has_verdict() {
        let p = plan();
        assert!(p.contains("tidur"), "{p}");
    }
    #[test]
    fn helpers() {
        assert_eq!(dur(450), "7j 30m");
        let d = Local::now().date_naive();
        assert_eq!(hm(at(d, "24:00").unwrap()), "00:00");
    }
}

/// "Kalau aku tidur sekarang": jam bangun, apakah masih sempat sebelum komitmen pertama, dan apa yang terlewat.
pub fn jika_tidur_sekarang() -> String {
    let now = Local::now();
    let s = assistant::schedule();
    let lie = now + Duration::minutes(15);
    let wake = lie + Duration::minutes((s.sleep_hours * 60.0) as i64);
    let (items, explore) = commitments(now);
    let mut out = format!("🛌 Kalau tidur sekarang: berbaring ±{} → bangun {} ({} jam, {}).\n", hm(lie), hm(wake), s.sleep_hours, day_name(wake, now));
    let missed: Vec<String> = items.iter().filter(|i| i.start > lie && i.start < wake && !i.label.starts_with("Jam kerja")).map(|i| format!("{} {}", hm(i.start), i.label)).chain(explore.iter().filter(|e| e.end > lie && e.start < wake).map(|e| e.label.clone())).take(5).collect();
    if !missed.is_empty() {
        out.push_str(&format!("Terlewat selama tidur: {}.\n", missed.join("; ")));
    }
    match items.iter().filter(|i| i.start > lie).min_by_key(|i| i.start) {
        Some(f) => {
            let slack = (f.start - Duration::minutes(PREP_MIN) - wake).num_minutes();
            if slack >= 0 {
                out.push_str(&format!("✅ Masih sempat: komitmen pertama {} {} ({}); sisa waktu siap-siap {}.", day_name(f.start, now), hm(f.start), f.label, dur(slack + PREP_MIN)));
            } else {
                out.push_str(&format!("⚠ Komitmen pertama {} {} ({}) terlalu dekat: kurang {} dari waktu bangun + siap-siap. Bangun paling lambat {}.", day_name(f.start, now), hm(f.start), f.label, dur(-slack), hm(f.start - Duration::minutes(PREP_MIN))));
            }
        }
        None => out.push_str("Tidak ada komitmen terjadwal setelah itu — tidurmu bebas. 🛏"),
    }
    out
}
