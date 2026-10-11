//! Perkakas lokal (tanpa model, tanpa internet): kalkulator, konversi satuan, zona waktu, hukum Ohm/arus 3 fase, selisih hari,
//! kata sandi acak, dan catatan cepat. Terinspirasi server alat MCP (time, sequential tools) dan dibuat ulang dalam Rust.
use chrono::{Datelike, Duration, Local, NaiveDate, Timelike, Utc};

// ---------------- kalkulator ----------------

struct P<'a> {
    s: &'a [u8],
    i: usize,
}

impl P<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() && (self.s[self.i] as char).is_whitespace() {
            self.i += 1;
        }
    }
    fn eat(&mut self, c: u8) -> bool {
        self.ws();
        if self.i < self.s.len() && self.s[self.i] == c {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn expr(&mut self) -> Result<f64, String> {
        let mut v = self.term()?;
        loop {
            if self.eat(b'+') {
                v += self.term()?;
            } else if self.eat(b'-') {
                v -= self.term()?;
            } else {
                return Ok(v);
            }
        }
    }
    fn term(&mut self) -> Result<f64, String> {
        let mut v = self.pow()?;
        loop {
            if self.eat(b'*') {
                v *= self.pow()?;
            } else if self.eat(b'/') {
                let d = self.pow()?;
                if d == 0.0 {
                    return Err("pembagian dengan nol".into());
                }
                v /= d;
            } else if self.eat(b'%') {
                let d = self.pow()?;
                v %= d;
            } else {
                return Ok(v);
            }
        }
    }
    fn pow(&mut self) -> Result<f64, String> {
        let b = self.unary()?;
        if self.eat(b'^') {
            return Ok(b.powf(self.pow()?));
        }
        Ok(b)
    }
    fn unary(&mut self) -> Result<f64, String> {
        if self.eat(b'-') {
            return Ok(-self.unary()?);
        }
        if self.eat(b'+') {
            return self.unary();
        }
        self.atom()
    }
    fn atom(&mut self) -> Result<f64, String> {
        self.ws();
        if self.eat(b'(') {
            let v = self.expr()?;
            if !self.eat(b')') {
                return Err("kurung tidak tertutup".into());
            }
            return Ok(v);
        }
        let st = self.i;
        while self.i < self.s.len() && ((self.s[self.i] as char).is_ascii_digit() || self.s[self.i] == b'.') {
            self.i += 1;
        }
        if self.i > st {
            return std::str::from_utf8(&self.s[st..self.i]).unwrap_or("").parse().map_err(|_| "angka tidak valid".to_string());
        }
        while self.i < self.s.len() && (self.s[self.i] as char).is_ascii_alphabetic() {
            self.i += 1;
        }
        let name = std::str::from_utf8(&self.s[st..self.i]).unwrap_or("").to_lowercase();
        match name.as_str() {
            "pi" => return Ok(std::f64::consts::PI),
            "e" => return Ok(std::f64::consts::E),
            "" => return Err("ekspresi tidak lengkap".into()),
            _ => {}
        }
        if !self.eat(b'(') {
            return Err(format!("'{name}' tidak dikenal"));
        }
        let a = self.expr()?;
        if !self.eat(b')') {
            return Err("kurung tidak tertutup".into());
        }
        let rad = a.to_radians();
        Ok(match name.as_str() {
            "sqrt" | "akar" => a.sqrt(),
            "sin" => rad.sin(),
            "cos" => rad.cos(),
            "tan" => rad.tan(),
            "ln" => a.ln(),
            "log" => a.log10(),
            "abs" => a.abs(),
            "exp" => a.exp(),
            _ => return Err(format!("fungsi '{name}' tidak dikenal")),
        })
    }
}

/// "3x4" -> "3*4", tetapi huruf x di dalam nama fungsi (exp) dibiarkan.
fn kali_x(s: &str) -> String {
    let c: Vec<char> = s.chars().collect();
    (0..c.len()).map(|i| if c[i] == 'x' && i > 0 && i + 1 < c.len() && c[i - 1].is_ascii_digit() && c[i + 1].is_ascii_digit() { '*' } else { c[i] }).collect()
}

pub fn hitung(expr: &str) -> String {
    let e = kali_x(expr).replace('×', "*").replace('÷', "/").replace("kali", "*").replace("bagi", "/").replace("tambah", "+").replace("kurang", "-").replace("pangkat", "^").replace(',', ".");
    let mut p = P { s: e.as_bytes(), i: 0 };
    match p.expr() {
        Ok(v) if p.i >= e.trim_end().len() => {
            let t = if v.fract().abs() < 1e-9 && v.abs() < 1e15 { format!("{}", v as i64) } else { format!("{:.6}", v).trim_end_matches('0').trim_end_matches('.').to_string() };
            format!("🧮 {} = {t}  (sudut sin/cos/tan dalam derajat)", expr.trim())
        }
        Ok(_) => "Ekspresi tidak utuh. Contoh: hitung 12*(3+4)^2/5 atau hitung sqrt(2)*pi".into(),
        Err(m) => format!("Tidak bisa menghitung: {m}. Contoh: hitung 12*(3+4)^2/5"),
    }
}

// ---------------- konversi satuan ----------------

fn unit(u: &str) -> Option<(&'static str, f64)> {
    let u = u.trim().to_lowercase();
    let t: &[(&[&str], &str, f64)] = &[
        (&["mm"], "panjang", 0.001), (&["cm"], "panjang", 0.01), (&["m", "meter"], "panjang", 1.0), (&["km", "kilometer"], "panjang", 1000.0),
        (&["inci", "inch", "in"], "panjang", 0.0254), (&["kaki", "ft", "feet"], "panjang", 0.3048), (&["yard", "yd"], "panjang", 0.9144), (&["mil", "mile"], "panjang", 1609.344),
        (&["mg"], "massa", 1e-6), (&["g", "gram"], "massa", 0.001), (&["kg", "kilogram"], "massa", 1.0), (&["ton"], "massa", 1000.0), (&["pon", "lb", "pound"], "massa", 0.45359237), (&["ons"], "massa", 0.1), (&["oz"], "massa", 0.0283495),
        (&["ml"], "volume", 0.001), (&["l", "liter"], "volume", 1.0), (&["galon", "gallon"], "volume", 3.78541), (&["m3"], "volume", 1000.0),
        (&["detik", "s", "sekon"], "waktu", 1.0), (&["menit"], "waktu", 60.0), (&["jam"], "waktu", 3600.0), (&["hari"], "waktu", 86400.0), (&["minggu"], "waktu", 604800.0),
        (&["w", "watt"], "daya", 1.0), (&["kw"], "daya", 1000.0), (&["mw"], "daya", 1e6), (&["hp"], "daya", 745.7), (&["pk"], "daya", 735.5),
        (&["j", "joule"], "energi", 1.0), (&["kj"], "energi", 1000.0), (&["wh"], "energi", 3600.0), (&["kwh"], "energi", 3.6e6), (&["kal"], "energi", 4.184), (&["kkal"], "energi", 4184.0),
        (&["pa"], "tekanan", 1.0), (&["kpa"], "tekanan", 1000.0), (&["bar"], "tekanan", 1e5), (&["psi"], "tekanan", 6894.757), (&["atm"], "tekanan", 101325.0), (&["mmhg"], "tekanan", 133.322),
        (&["m/s"], "kecepatan", 1.0), (&["km/jam", "kmh", "km/h"], "kecepatan", 1.0 / 3.6), (&["mph"], "kecepatan", 0.44704), (&["knot"], "kecepatan", 0.514444),
        (&["b", "byte"], "data", 1.0), (&["kb"], "data", 1024.0), (&["mb"], "data", 1048576.0), (&["gb"], "data", 1073741824.0), (&["tb"], "data", 1099511627776.0),
        (&["m2"], "luas", 1.0), (&["ha", "hektar"], "luas", 10000.0), (&["km2"], "luas", 1e6), (&["acre"], "luas", 4046.86),
    ];
    t.iter().find(|(n, _, _)| n.contains(&u.as_str())).map(|(_, c, f)| (*c, *f))
}

/// "5 km ke mil" / "100 f ke c"
pub fn konversi(text: &str) -> String {
    let l = text.to_lowercase().replace(',', ".");
    let toks: Vec<&str> = l.split_whitespace().collect();
    let Some(ix) = toks.iter().position(|t| t.parse::<f64>().is_ok()) else { return "Format: konversi <angka> <satuan> ke <satuan>, mis. konversi 5 km ke mil".into() };
    let val: f64 = toks[ix].parse().unwrap_or(0.0);
    let rest: Vec<&str> = toks[ix + 1..].iter().copied().filter(|t| !["ke", "jadi", "dalam", "menjadi", "berapa", "to", "=", "sama"].contains(t)).collect();
    let (Some(a), Some(b)) = (rest.first(), rest.get(1)) else { return "Format: konversi <angka> <satuan> ke <satuan>, mis. konversi 5 km ke mil".into() };
    // suhu
    let temp = |s: &str| matches!(s.trim_start_matches('°'), "c" | "f" | "k" | "celsius" | "fahrenheit" | "kelvin");
    if temp(a) && temp(b) {
        let n = |s: &str| s.trim_start_matches('°').chars().next().unwrap_or('c');
        let c = match n(a) { 'f' => (val - 32.0) * 5.0 / 9.0, 'k' => val - 273.15, _ => val };
        let r = match n(b) { 'f' => c * 9.0 / 5.0 + 32.0, 'k' => c + 273.15, _ => c };
        return format!("🔁 {val} °{} = {:.2} °{}", n(a).to_ascii_uppercase(), r, n(b).to_ascii_uppercase());
    }
    match (unit(a), unit(b)) {
        (Some((ca, fa)), Some((cb, fb))) if ca == cb => format!("🔁 {val} {a} = {} {b}  ({ca})", fmt(val * fa / fb)),
        (Some((ca, _)), Some((cb, _))) => format!("Tidak bisa mengonversi {ca} ({a}) ke {cb} ({b}): jenis besarannya berbeda."),
        _ => format!("Satuan '{a}' atau '{b}' belum dikenal. Didukung: panjang, massa, volume, waktu, daya, energi, tekanan, kecepatan, data, luas, suhu (c/f/k)."),
    }
}

fn fmt(v: f64) -> String {
    if v.abs() >= 1e6 || (v.abs() < 1e-3 && v != 0.0) { format!("{v:.4e}") } else { format!("{:.4}", v).trim_end_matches('0').trim_end_matches('.').to_string() }
}

// ---------------- zona waktu ----------------

fn nth_sunday(y: i32, m: u32, n: u32) -> NaiveDate {
    let first = NaiveDate::from_ymd_opt(y, m, 1).unwrap();
    let off = (7 - first.weekday().num_days_from_sunday()) % 7;
    first + Duration::days((off + 7 * (n - 1)) as i64)
}

fn last_sunday(y: i32, m: u32) -> NaiveDate {
    let next = if m == 12 { NaiveDate::from_ymd_opt(y + 1, 1, 1) } else { NaiveDate::from_ymd_opt(y, m + 1, 1) }.unwrap();
    let last = next - Duration::days(1);
    last - Duration::days(last.weekday().num_days_from_sunday() as i64)
}

/// Offset menit dari UTC untuk zona (dengan aturan DST EU/AS/AU sederhana).
fn offset(zone: &str, now: chrono::DateTime<Utc>) -> Option<i32> {
    let (y, d) = (now.year(), now.date_naive());
    let eu = d >= last_sunday(y, 3) && d < last_sunday(y, 10);
    let us = d >= nth_sunday(y, 3, 2) && d < nth_sunday(y, 11, 1);
    let au = d >= nth_sunday(y, 10, 1) || d < nth_sunday(y, 4, 1);
    Some(match zone {
        "wib" | "jakarta" | "bandung" | "surabaya" | "medan" | "semarang" | "yogyakarta" | "palembang" | "bangkok" | "hanoi" => 420,
        "wita" | "bali" | "makassar" | "denpasar" | "singapura" | "singapore" | "kuala lumpur" | "malaysia" | "manila" | "beijing" | "shanghai" | "hongkong" | "perth" | "taipei" => 480,
        "wit" | "jayapura" | "ambon" | "tokyo" | "jepang" | "osaka" | "seoul" | "korea" => 540,
        "london" | "inggris" | "utc" | "gmt" | "lisbon" => if eu && zone != "utc" && zone != "gmt" { 60 } else { 0 },
        "paris" | "berlin" | "belanda" | "amsterdam" | "madrid" | "roma" | "jerman" | "prancis" => if eu { 120 } else { 60 },
        "dubai" | "uae" | "abu dhabi" => 240,
        "riyadh" | "mekah" | "makkah" | "madinah" | "jeddah" | "arab saudi" | "moskow" | "istanbul" => 180,
        "india" | "delhi" | "mumbai" => 330,
        "sydney" | "melbourne" | "canberra" => if au { 660 } else { 600 },
        "new york" | "washington" | "toronto" | "miami" => if us { -240 } else { -300 },
        "chicago" | "mexico" => if us { -300 } else { -360 },
        "los angeles" | "san francisco" | "seattle" | "california" => if us { -420 } else { -480 },
        _ => return None,
    })
}

const ZONES: &[&str] = &["los angeles", "san francisco", "new york", "washington", "abu dhabi", "kuala lumpur", "arab saudi", "yogyakarta", "california", "melbourne", "singapura", "singapore", "palembang", "surabaya", "semarang", "makassar", "denpasar", "istanbul", "amsterdam", "bandung", "jakarta", "jayapura", "shanghai", "hongkong", "malaysia", "manila", "beijing", "bangkok", "jerman", "prancis", "inggris", "korea", "jepang", "tokyo", "osaka", "seoul", "london", "berlin", "madrid", "moskow", "riyadh", "jeddah", "mekah", "makkah", "madinah", "dubai", "india", "delhi", "mumbai", "sydney", "canberra", "toronto", "chicago", "mexico", "seattle", "miami", "perth", "taipei", "hanoi", "lisbon", "ambon", "medan", "paris", "roma", "belanda", "uae", "bali", "wita", "wib", "wit", "utc", "gmt"];

fn find_zones(l: &str) -> Vec<(usize, &'static str)> {
    let mut v: Vec<(usize, &str)> = vec![];
    let mut taken: Vec<(usize, usize)> = vec![];
    for z in ZONES {
        let mut from = 0;
        while let Some(i) = l[from..].find(z) {
            let a = from + i;
            let b = a + z.len();
            let ok = (a == 0 || !l.as_bytes()[a - 1].is_ascii_alphabetic()) && (b >= l.len() || !l.as_bytes()[b].is_ascii_alphabetic());
            if ok && !taken.iter().any(|(s, e)| a < *e && b > *s) {
                taken.push((a, b));
                v.push((a, z));
            }
            from = b;
        }
    }
    v.sort();
    v
}

fn label(off: i32) -> String {
    format!("UTC{}{:02}:{:02}", if off < 0 { "-" } else { "+" }, off.abs() / 60, off.abs() % 60)
}

/// "jam di tokyo" / "jam 14:00 wib di london" / "konversi 09:30 london ke jakarta"
pub fn zona(text: &str) -> String {
    let l = text.to_lowercase();
    let zs = find_zones(&l);
    let now = Utc::now();
    // jam eksplisit?
    let time = l.split_whitespace().find_map(|w| {
        let w = w.trim_matches(|c: char| !c.is_ascii_digit() && c != ':' && c != '.');
        let (h, m) = w.split_once(|c| c == ':' || c == '.')?;
        Some((h.parse::<i64>().ok().filter(|h| *h < 24)?, m.parse::<i64>().ok().filter(|m| *m < 60)?))
    });
    if let Some((h, m)) = time {
        let src = zs.first().map(|z| z.1).unwrap_or("wib");
        let dst: Vec<&str> = zs.iter().skip(if zs.is_empty() { 0 } else { 1 }).map(|z| z.1).collect();
        let (so, Some(_)) = (offset(src, now).unwrap_or(420), Some(())) else { return String::new() };
        let utc_min = h * 60 + m - so as i64;
        let mut out = format!("🕐 {h:02}:{m:02} {src} ({}) =", label(so));
        let targets: Vec<&str> = if dst.is_empty() { vec!["wib", "london", "new york"] } else { dst };
        for t in targets {
            if let Some(to) = offset(t, now) {
                let tm = (utc_min + to as i64).rem_euclid(1440);
                let day = (utc_min + to as i64).div_euclid(1440);
                out.push_str(&format!("\n• {t}: {:02}:{:02}{} ({})", tm / 60, tm % 60, match day { 1 => " (+1 hari)", -1 => " (-1 hari)", _ => "" }, label(to)));
            }
        }
        return out;
    }
    let list: Vec<&str> = if zs.is_empty() { vec!["wib", "wita", "wit", "london", "new york", "tokyo"] } else { zs.iter().map(|z| z.1).collect() };
    let mut out = String::from("🕐 Waktu sekarang:");
    for z in list {
        if let Some(o) = offset(z, now) {
            let t = now + Duration::minutes(o as i64);
            out.push_str(&format!("\n• {z}: {:02}:{:02}, {} ({})", t.hour(), t.minute(), t.format("%d/%m"), label(o)));
        }
    }
    out
}

// ---------------- Ohm & arus 3 fase ----------------

fn num_after(l: &str, keys: &[&str]) -> Option<f64> {
    for k in keys {
        let mut from = 0;
        while let Some(i) = l[from..].find(k) {
            let a = from + i;
            let pre_ok = a == 0 || !l.as_bytes()[a - 1].is_ascii_alphabetic();
            let rest = l[a + k.len()..].trim_start_matches(|c: char| c == '=' || c == ':' || c == ' ' || c == '-');
            let n: String = rest.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
            if pre_ok && !n.is_empty() {
                let mut v: f64 = n.parse().ok()?;
                let tail = rest[n.len()..].trim_start();
                if tail.starts_with("kw") || tail.starts_with("kv") || tail.starts_with("ka") || tail.starts_with("kohm") || tail.starts_with("k") && !tail.starts_with("kw") && keys.contains(&"r") { v *= 1000.0; }
                if tail.starts_with("ma") || tail.starts_with("mw") { v /= 1000.0; }
                return Some(v);
            }
            from = a + k.len();
        }
    }
    None
}

pub fn ohm(text: &str) -> String {
    let l = text.to_lowercase();
    let (mut v, mut i, mut r, mut p) = (num_after(&l, &["v", "tegangan", "volt"]), num_after(&l, &["i", "arus", "ampere"]), num_after(&l, &["r", "hambatan", "resistansi"]), num_after(&l, &["p", "daya", "watt"]));
    for _ in 0..3 {
        if v.is_none() { v = i.zip(r).map(|(i, r)| i * r).or(p.zip(i).map(|(p, i)| p / i)).or(p.zip(r).map(|(p, r)| (p * r).sqrt())); }
        if i.is_none() { i = v.zip(r).map(|(v, r)| v / r).or(p.zip(v).map(|(p, v)| p / v)).or(p.zip(r).map(|(p, r)| (p / r).sqrt())); }
        if r.is_none() { r = v.zip(i).map(|(v, i)| v / i).or(v.zip(p).map(|(v, p)| v * v / p)).or(p.zip(i).map(|(p, i)| p / (i * i))); }
        if p.is_none() { p = v.zip(i).map(|(v, i)| v * i).or(v.zip(r).map(|(v, r)| v * v / r)).or(i.zip(r).map(|(i, r)| i * i * r)); }
    }
    match (v, i, r, p) {
        (Some(v), Some(i), Some(r), Some(p)) if [v, i, r, p].iter().all(|x| x.is_finite()) => format!("⚡ Hukum Ohm: V = {} V · I = {} A · R = {} Ω · P = {} W", fmt(v), fmt(i), fmt(r), fmt(p)),
        _ => "Beri minimal dua nilai, mis. ohm V=12 R=4 atau ohm P=60 V=220 (tegangan, arus, hambatan, daya; 'k' = ribu).".into(),
    }
}

/// Arus 3 fase: I = P / (√3 · V · cosφ)
pub fn fase3(text: &str) -> String {
    let l = text.to_lowercase();
    let p = num_after(&l, &["p", "daya"]);
    let v = num_after(&l, &["v", "tegangan"]).unwrap_or(380.0);
    let cos = num_after(&l, &["cos", "pf", "cosφ"]).unwrap_or(0.85);
    let Some(mut p) = p else { return "Beri daya, mis. fase3 P=15kw V=380 cos=0.85 (bawaan V=380, cosφ=0.85)".into() };
    if l.contains("kw") && p < 1000.0 { p *= 1000.0; }
    let i = p / (3f64.sqrt() * v * cos);
    format!("⚡ 3 fase: P = {} W, V = {} V, cosφ = {cos} → I = {} A. Pilih pengaman ≥ 1,25 × I ≈ {} A; verifikasi dengan PUIL/SNI sebelum dipasang.", fmt(p), fmt(v), fmt(i), fmt(i * 1.25))
}

// ---------------- selisih hari ----------------

fn bulan(s: &str) -> Option<u32> {
    ["januari", "februari", "maret", "april", "mei", "juni", "juli", "agustus", "september", "oktober", "november", "desember"].iter().position(|m| m.starts_with(&s[..s.len().min(3)]) && s.len() >= 3).map(|i| i as u32 + 1)
}

pub fn hari(text: &str) -> String {
    let l = text.to_lowercase().replace('/', "-");
    let today = Local::now().date_naive();
    let toks: Vec<&str> = l.split(|c: char| c.is_whitespace() || c == ',').filter(|t| !t.is_empty()).collect();
    let mut date: Option<NaiveDate> = None;
    for (i, t) in toks.iter().enumerate() {
        if t.contains('-') {
            let p: Vec<i32> = t.split('-').filter_map(|x| x.parse().ok()).collect();
            if p.len() == 3 {
                date = if p[0] > 31 { NaiveDate::from_ymd_opt(p[0], p[1] as u32, p[2] as u32) } else { NaiveDate::from_ymd_opt(p[2], p[1] as u32, p[0] as u32) };
            }
        } else if let (Ok(d), Some(m)) = (t.parse::<u32>(), toks.get(i + 1).and_then(|x| bulan(x))) {
            let y = toks.get(i + 2).and_then(|y| y.parse::<i32>().ok()).filter(|y| *y > 1900);
            date = match y {
                Some(y) => NaiveDate::from_ymd_opt(y, m, d),
                None => NaiveDate::from_ymd_opt(today.year(), m, d).map(|x| if x < today { NaiveDate::from_ymd_opt(today.year() + 1, m, d).unwrap_or(x) } else { x }),
            };
        }
        if date.is_some() { break; }
    }
    let Some(d) = date else { return "Sebut tanggalnya, mis. 'berapa hari lagi ke 17 agustus' atau 'hari 25-12-2026'.".into() };
    let n = (d - today).num_days();
    let nama = ["Minggu", "Senin", "Selasa", "Rabu", "Kamis", "Jumat", "Sabtu"][d.weekday().num_days_from_sunday() as usize];
    format!("📅 {} {} ({}): {}", nama, d.format("%d/%m/%Y"), if n == 0 { "hari ini".to_string() } else { format!("{} {}", n.abs(), if n > 0 { "hari lagi" } else { "hari yang lalu" }) }, if n.abs() >= 14 { format!("≈ {} minggu", n.abs() / 7) } else { String::new() })
}

// ---------------- kata sandi acak ----------------

pub fn password(arg: &str) -> String {
    use rand::Rng;
    let n: usize = arg.split_whitespace().find_map(|w| w.parse().ok()).unwrap_or(16).clamp(8, 64);
    let cs: Vec<char> = "abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789!@#$%^*-_=+".chars().collect();
    let mut rng = rand::thread_rng();
    let p: String = (0..n).map(|_| cs[rng.gen_range(0..cs.len())]).collect();
    format!("🔐 Kata sandi acak ({n} karakter): {p}\nSalin lalu simpan di pengelola kata sandi; NOVA tidak menyimpannya.")
}

// ---------------- catatan cepat ----------------

fn ndb() -> Option<rusqlite::Connection> {
    let c = crate::assistant::db().ok()?;
    c.execute_batch("CREATE TABLE IF NOT EXISTS catatan(id INTEGER PRIMARY KEY, ts INT, teks TEXT);").ok()?;
    Some(c)
}

pub fn catat(teks: &str) -> String {
    let t = teks.trim();
    if t.is_empty() {
        return "Catat apa? Contoh: catat beli kabel NYM 2x1.5 besok".into();
    }
    let Some(c) = ndb() else { return "Catatan tidak tersedia.".into() };
    let _ = c.execute("INSERT INTO catatan(ts, teks) VALUES(?1, ?2)", rusqlite::params![Local::now().timestamp(), t]);
    let id = c.last_insert_rowid();
    format!("📝 Dicatat (#{id}): {t}")
}

pub fn catatan(arg: &str) -> String {
    let Some(c) = ndb() else { return "Catatan tidak tersedia.".into() };
    let a = arg.trim().to_lowercase();
    if let Some(n) = a.strip_prefix("hapus ").and_then(|x| x.trim().trim_start_matches('#').parse::<i64>().ok()) {
        return format!("🗑 {} catatan dihapus.", c.execute("DELETE FROM catatan WHERE id=?1", [n]).unwrap_or(0));
    }
    let like = format!("%{}%", if a == "semua" || a.is_empty() { "" } else { &a });
    let Ok(mut st) = c.prepare("SELECT id, ts, teks FROM catatan WHERE lower(teks) LIKE ?1 ORDER BY id DESC LIMIT 12") else { return String::new() };
    let rows: Vec<(i64, i64, String)> = st.query_map([like], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).map(|r| r.flatten().collect()).unwrap_or_default();
    if rows.is_empty() {
        return "Belum ada catatan. Tambah: catat <teks>".into();
    }
    let mut s = String::from("📒 Catatan:\n");
    for (id, ts, t) in rows {
        s.push_str(&format!("#{id} {} — {t}\n", chrono::DateTime::from_timestamp(ts, 0).map(|d| d.with_timezone(&Local).format("%d/%m %H:%M").to_string()).unwrap_or_default()));
    }
    s.push_str("Hapus: catatan hapus <nomor>");
    s
}

// ---------------- aturan bahasa alami ----------------

/// Kembalikan (alat, argumen) bila kalimat jelas meminta salah satu perkakas ini.
pub fn rule(s: &str, l: &str) -> Option<(&'static str, String)> {
    let math_only = |t: &str| !t.trim().is_empty() && t.chars().all(|c| c.is_ascii_digit() || " +-*/^%().,x×÷".contains(c)) && t.chars().any(|c| c.is_ascii_digit());
    if let Some(r) = l.strip_prefix("hitung ").or_else(|| l.strip_prefix("berapa ")) {
        let r = r.trim_end_matches('?');
        if math_only(r) || (r.contains("sqrt") || r.contains("sin(") || r.contains("cos(") || r.contains("pi")) && r.chars().any(|c| c.is_ascii_digit() || c == 'p') && !r.contains("tegangan") && !r.contains("drop") {
            return Some(("hitung", r.to_string()));
        }
    }
    if math_only(l.trim_end_matches('?')) && l.chars().any(|c| "+-*/^%×÷".contains(c)) && l.chars().filter(|c| c.is_ascii_digit()).count() >= 2 {
        return Some(("hitung", l.trim_end_matches('?').to_string()));
    }
    let toks: Vec<&str> = l.split_whitespace().collect();
    let is_unit = |t: &str| unit(t).is_some() || matches!(t.trim_start_matches('°'), "c" | "f" | "k" | "celsius" | "fahrenheit" | "kelvin");
    if let Some(ix) = toks.iter().position(|t| t.replace(',', ".").parse::<f64>().is_ok()) {
        if l.starts_with("konversi ") && toks.len() > ix + 2 || (toks.len() > ix + 3 && ["ke", "jadi", "menjadi", "dalam"].contains(&toks[ix + 2]) && is_unit(toks[ix + 1]) && is_unit(toks[ix + 3])) {
            return Some(("konversi", s.to_string()));
        }
    }
    if (l.contains("jam berapa di ") || l.contains("jam di ") || l.contains("waktu di ") || l.contains("konversi waktu") || l.contains("konversi jam") || l.contains("zona waktu") || (l.contains("jam ") && l.contains(" di ") && !find_zones(l).is_empty())) && !find_zones(l).is_empty() || l.contains("zona waktu") {
        return Some(("zona", s.to_string()));
    }
    if l.contains("http") && (l.contains("ringkas") || l.contains("rangkum") || l.contains("isi halaman") || l.contains("baca halaman")) && !l.contains("buka ") {
        return Some(("halaman", s.to_string()));
    }
    if l.contains("ingatkan") && (l.contains("hujan") || l.contains("cuaca") || l.contains("panas")) || l.starts_with("cuacapantau") || l.starts_with("pantau cuaca") {
        return Some(("cuacapantau", s.to_string()));
    }
    if l.starts_with("pantau ") && !l.contains("cuaca") || l.contains("kabari kalau harga") || l.contains("ingatkan kalau harga") || l == "pantauan" || l == "pantau" {
        return Some(("pantau", s.to_string()));
    }
    if (l.starts_with("harga ") || l.contains("grafik ") || l.contains("harga saham") || l.starts_with("kurs dolar") || l.contains("chart ")) && !l.contains("kurs 1") && !l.contains("pantau") {
        return Some(("harga", s.to_string()));
    }
    if l.contains(".csv") || l.contains(".xlsx") || l.contains("analisis csv") || l.contains("analisis excel") {
        return Some(("csv", s.to_string()));
    }
    if l.starts_with("gitringkas") || l.contains("ringkas git") || l.contains("git ringkas") || l.contains("apa yang berubah di repo") || l.contains("status git") || l.contains("ringkasan git") {
        let p = s.split_whitespace().find(|w| w.contains(':') && (w.contains('\\') || w.contains('/'))).unwrap_or("");
        return Some(("gitringkas", p.to_string()));
    }
    if l.starts_with("ohm ") || l.contains("hukum ohm") {
        return Some(("ohm", s.to_string()));
    }
    if l.starts_with("fase3") || l.starts_with("arus 3 fase") || l.contains("arus tiga fase") || l.contains("arus 3 fasa") {
        return Some(("fase3", s.to_string()));
    }
    if l.contains("hari lagi") || l.starts_with("hari ") && l.chars().any(|c| c.is_ascii_digit()) || l.contains("selisih hari") {
        return Some(("hari", s.to_string()));
    }
    if l.contains("password") || l.contains("kata sandi acak") || l.contains("sandi acak") {
        if ["buat", "bikin", "generate", "acak", "baru"].iter().any(|k| l.contains(k)) {
            return Some(("password", s.to_string()));
        }
    }
    for pre in ["catat ", "catatan hapus ", "tolong catat "] {
        if l.starts_with(pre) {
            return Some(if pre == "catatan hapus " { ("catatan", format!("hapus {}", &s[pre.len()..])) } else { ("catat", s[pre.len()..].to_string()) });
        }
    }
    if l == "catatan" || l.starts_with("catatan ") || l.contains("lihat catatan") || l.contains("daftar catatan") {
        return Some(("catatan", l.replace("lihat catatan", "").replace("daftar catatan", "").replace("catatan", "").trim().to_string()));
    }
    if l.starts_with("cuaca") || l.contains("cuaca di ") || l.contains("prakiraan cuaca") || l.contains("ramalan cuaca") {
        return Some(("cuaca", s.to_string()));
    }
    if l.starts_with("kurs ") || l.contains("kurs ") || (toks.len() >= 4 && toks[0].parse::<f64>().is_ok() && ["ke", "jadi", "dalam"].contains(&toks[2]) && toks[1].len() == 3 && toks[3].len() == 3) {
        return Some(("kurs", s.to_string()));
    }
    for pre in ["wikipedia ", "wiki ", "siapa itu ", "apa itu "] {
        if l.starts_with(pre) && pre.starts_with('w') {
            return Some(("wiki", s[pre.len()..].to_string()));
        }
    }
    if l.starts_with("terjemahkan ") || l.starts_with("terjemah ") || l.starts_with("translate ") {
        return Some(("terjemah", s.to_string()));
    }
    if ["langkah demi langkah", "pikirkan ", "berpikir "].iter().any(|k| l.contains(k)) && toks.len() >= 4 {
        return Some(("berpikir", s.to_string()));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn calculator() {
        assert!(hitung("12*(3+4)^2/5").contains("= 117.6"));
        assert!(hitung("sqrt(16)+2").contains("= 6"));
        assert!(hitung("1/0").contains("nol"));
        assert!(hitung("exp(0)+2x3").contains("= 7"));
    }
    #[test]
    fn units() {
        assert!(konversi("5 km ke m").contains("5000"));
        assert!(konversi("100 c ke f").contains("212"));
        assert!(konversi("5 kg ke m").contains("berbeda"));
    }
    #[test]
    fn ohm_and_phase() {
        assert!(ohm("ohm V=12 R=4").contains("I = 3 A"));
        assert!(ohm("ohm P=60 V=220").contains("I = 0.2727"));
        assert!(fase3("fase3 P=15kw V=380 cos=0.85").contains("I = 26.8"));
    }
    #[test]
    fn zones_and_dates() {
        assert!(zona("jam 14:00 wib di tokyo").contains("16:00"));
        assert!(hari("25-12-2099").contains("2099"));
        assert_eq!(rule("hitung 2+3*4", "hitung 2+3*4").unwrap().0, "hitung");
        assert!(rule("hitung drop tegangan panjang 50 m", "hitung drop tegangan panjang 50 m").is_none());
        assert_eq!(rule("cuaca di bandung", "cuaca di bandung").unwrap().0, "cuaca");
    }
}
