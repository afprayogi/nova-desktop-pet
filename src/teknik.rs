//! Ilmu kelistrikan & engineering: kalkulator pasti + basis pengetahuan lokal (SQLite FTS5) + belajar dari PDF.
//! Catatan bawaan adalah ringkasan umum; angka keselamatan WAJIB diverifikasi ke PUIL 2011 / SNI / standar resmi.
use crate::config;
use rusqlite::Connection;

const WARN: &str = "\n⚠ Indikatif. Verifikasi ke PUIL 2011/SNI/standar resmi sebelum dipakai di lapangan.";

const KB: &[(&str, &str)] = &[
    ("Hukum Ohm", "V = I x R. Daya P = V x I = I^2 x R = V^2 / R. Satuan: V volt, I ampere, R ohm, P watt."),
    ("Daya AC satu fasa", "P = V x I x cos(phi) (watt), S = V x I (VA), Q = V x I x sin(phi) (VAR). S^2 = P^2 + Q^2. Faktor daya = P/S."),
    ("Daya AC tiga fasa", "P = akar3 x V(L-L) x I x cos(phi). S = akar3 x V x I. Tegangan fasa-netral = V(L-L)/akar3 (380 V -> 220 V). Arus saluran = P / (akar3 x V x cos phi)."),
    ("Faktor daya dan kapasitor", "Perbaikan cos phi: Qc (kVAR) = P(kW) x (tan phi1 - tan phi2). Kapasitor mengurangi kVAR reaktif, arus saluran, dan rugi-rugi. Hindari kompensasi berlebih (leading) dan resonansi harmonik; gunakan reaktor detuned bila ada beban nonlinier."),
    ("Drop tegangan", "1 fasa: dV = 2 x rho x L x I / A. 3 fasa: dV = akar3 x rho x L x I x cos phi / A (reaktansi diabaikan). rho tembaga ~0,0175 ohm.mm2/m (20C), aluminium ~0,028. Umumnya batas drop tegangan beban 3-5%; periksa PUIL."),
    ("Kemampuan hantar arus (KHA) kabel", "Kabel dipilih agar KHA (Iz) >= arus beban (Ib) setelah faktor koreksi suhu, pengelompokan, dan cara pemasangan. Pengaman In memenuhi Ib <= In <= Iz. Cek tabel KHA PUIL 2011 / IEC 60364-5-52."),
    ("MCB, MCCB, ELCB/RCD", "MCB: proteksi beban lebih (termal) dan hubung singkat (magnetik); kurva B/C/D menentukan batas trip magnetik. MCCB untuk arus lebih besar dan dapat diatur. RCD/ELCB mendeteksi arus bocor ke tanah (30 mA untuk proteksi manusia) dan TIDAK menggantikan MCB."),
    ("Pentanahan (grounding)", "Fungsi: keselamatan (arus gangguan mengalir ke tanah, pengaman trip) dan referensi tegangan. Elektroda: batang, pelat, pita. Ukur tahanan dengan earth tester. Semua bagian konduktif terbuka dihubungkan ke PE. Nilai tahanan yang diizinkan tergantung sistem (TN/TT/IT): lihat PUIL."),
    ("Sistem TN, TT, IT", "TN: netral sumber ditanahkan, BKT ke netral (TN-S, TN-C-S). TT: BKT punya tanah sendiri, wajib RCD. IT: sumber tak ditanahkan/impedansi tinggi, perlu monitor isolasi."),
    ("Warna kabel instalasi", "Umum PUIL 2011: L1 merah, L2 kuning, L3 hitam, netral biru, PE kuning-hijau. Instalasi lama bisa berbeda; selalu uji dengan alat, jangan hanya percaya warna."),
    ("Keselamatan kerja listrik", "Selalu: matikan, kunci dan beri tanda (LOTO), uji tidak bertegangan dengan tester yang diverifikasi, tanahkan bila perlu, baru bekerja. Gunakan APD sesuai (sarung tangan isolasi, kacamata, sepatu safety). Bekerja bertegangan hanya oleh yang berwenang."),
    ("Hubung singkat", "Arus hubung singkat = V / Z total loop. Pengaman harus mampu memutus (breaking capacity Icu/Icn) lebih besar dari arus hubung singkat di titik pemasangan, dan waktu trip memenuhi syarat sentuh tak langsung."),
    ("Motor induksi", "Kecepatan sinkron ns = 120 x f / p (rpm). Slip s = (ns - n)/ns. Arus beban penuh 3 fasa I = P / (akar3 x V x eta x cos phi). Arus start DOL ~5-8 x In."),
    ("Metode start motor", "DOL (langsung) sederhana tapi arus start besar. Bintang-segitiga mengurangi arus start ~1/3 (motor harus delta di 380 V). Soft starter menaikkan tegangan bertahap. VSD/inverter mengatur frekuensi sehingga torsi dan kecepatan terkontrol dan arus start rendah."),
    ("Proteksi motor", "Perlu: pengaman hubung singkat (MCB/MCCB/fuse), relai beban lebih termal (TOR) atau MPCB, dan opsional proteksi fasa hilang/urutan fasa. Setel TOR sekitar arus nominal pelat nama."),
    ("Trafo", "Rasio V1/V2 = N1/N2 = I2/I1. Arus nominal I = S / (akar3 x V). Rugi besi tetap, rugi tembaga bergantung beban. Impedansi % menentukan arus hubung singkat sekunder: Isc = In / Z%."),
    ("Kontaktor dan relai", "Relai: saklar kecil dikendalikan kumparan, untuk sinyal kontrol. Kontaktor: untuk beban daya, kontak utama + bantu. Pilih kategori AC-1 (resistif), AC-3 (motor). Pasang peredam (snubber/dioda flyback) pada kumparan DC."),
    ("Harmonisa", "Beban nonlinier (VSD, UPS, LED driver) menyebabkan arus harmonik: panas berlebih di netral dan trafo, kapasitor rusak. Ukur THD; mitigasi dengan filter, trafo K-factor, reaktor."),
    ("UPS dan baterai", "Kapasitas baterai (Wh) = V x Ah. Waktu cadangan ~ (Wh x efisiensi x DoD) / beban W. Aki timbal-asam disarankan DoD <=50%. Pilih UPS dengan daya VA >= beban/ (pf) dengan margin 20-25%."),
    ("PLTS (solar PV)", "Daya panel (Wp) x jam matahari efektif (~4 jam di Indonesia) x efisiensi sistem (~0,75) = energi harian. String: jumlah modul seri menentukan Voc total (harus < batas inverter pada suhu dingin). Pasang proteksi DC, SPD, dan pentanahan."),
    ("Resistor, kapasitor, induktor", "R: menghambat arus. C: menyimpan muatan, Xc = 1/(2 pi f C). L: menyimpan energi magnet, XL = 2 pi f L. Seri R: Rt = R1+R2; paralel: 1/Rt = 1/R1+1/R2. Kapasitor seri kebalikannya."),
    ("Filter RC dan frekuensi cutoff", "fc = 1 / (2 pi R C). Low-pass: melewatkan frekuensi rendah; high-pass sebaliknya. Pada fc amplitudo turun ke 70,7% (-3 dB)."),
    ("LED dan resistor pembatas", "R = (Vsumber - Vf) / If. Contoh: 5 V, LED merah Vf 2 V, If 10 mA -> R = 300 ohm (pakai 330). Daya resistor = I^2 x R."),
    ("Dioda dan transistor", "Dioda: konduksi searah, Vf ~0,7 V (Si). Transistor BJT: arus basis mengendalikan Ic = beta x Ib. MOSFET: tegangan gate mengendalikan arus; cocok untuk saklar daya. Pasang dioda flyback pada beban induktif."),
    ("Op-amp", "Penguat inverting: Av = -Rf/Rin. Non-inverting: Av = 1 + Rf/Rin. Op-amp ideal: impedansi masuk tak hingga, input + dan - bertegangan sama pada umpan balik negatif."),
    ("PWM", "Pulse width modulation: duty cycle D = ton/T. Tegangan rata-rata = D x Vsumber. Dipakai untuk dimmer, kontrol kecepatan motor DC, dan konverter daya."),
    ("ADC dan resolusi", "Resolusi LSB = Vref / 2^n. ADC 10 bit, Vref 5 V -> 4,88 mV per langkah. Perhatikan teorema Nyquist: sampling >= 2x frekuensi sinyal tertinggi, dan beri filter anti-aliasing."),
    ("PLC dasar", "PLC bekerja siklus scan: baca input -> jalankan program -> tulis output. Bahasa IEC 61131-3: Ladder (LD), Function Block (FBD), Structured Text (ST), Instruction List, SFC. Input/output digital dan analog; perhatikan sumber (PNP) vs sink (NPN)."),
    ("Ladder logic", "Kontak NO/NC mewakili input; coil mewakili output. Rangkaian seal-in (self-holding): kontak coil paralel dengan tombol START, seri dengan STOP (NC) untuk menahan motor tetap hidup. Gunakan interlock agar dua output berlawanan tak aktif bersamaan."),
    ("Sinyal analog 4-20 mA", "4 mA = 0%, 20 mA = 100%. Nilai di bawah 4 mA menandakan kabel putus (live zero). Skala: nilai = (I - 4)/16 x rentang. Tahan noise dan jarak jauh lebih baik daripada 0-10 V."),
    ("Sensor umum", "Proximity induktif (logam), kapasitif (benda apa saja), fotoelektrik, limit switch, termokopel (tipe K ~41 uV/C), RTD PT100 (100 ohm pada 0C, ~0,385 ohm/C), load cell, encoder."),
    ("Komunikasi industri", "Modbus RTU (RS-485, master-slave, 9600-115200 bps, hingga 32 node tanpa repeater), Modbus TCP, Profibus, Profinet, EtherNet/IP, CAN. RS-485 butuh terminasi 120 ohm di ujung bus dan kabel twisted pair."),
    ("IP rating", "IP + 2 digit: digit1 proteksi padat (0-6), digit2 proteksi air (0-9). IP54: tahan debu terbatas dan cipratan; IP65: kedap debu dan semprotan air; IP67: dapat direndam sementara."),
    ("Konversi satuan daya", "1 HP = 746 W (mekanik), 1 kW = 1,341 HP, 1 PK ~ 735,5 W. kVA = kW / cos phi. 1 kWh = 1000 W selama 1 jam."),
    ("Resistivitas bahan", "Tembaga 0,0175 ohm.mm2/m, aluminium 0,028, besi ~0,10. Resistansi R = rho x L / A. Naik sekitar 0,4% per derajat C untuk tembaga."),
];

/// Penanaman data awal secara atomik (aman bila beberapa tugas membuka basis pengetahuan bersamaan).
fn seed(c: &Connection) -> rusqlite::Result<()> {
    let count = |src: &str| c.query_row("SELECT COUNT(*) FROM kb WHERE src=?1", [src], |r| r.get::<_, i64>(0));
    if count("bawaan")? > 0 && count("pemrograman")? > 0 {
        return Ok(());
    }
    c.execute_batch("BEGIN IMMEDIATE")?;
    let r = (|| -> rusqlite::Result<()> {
        if count("bawaan")? == 0 {
            for (t, b) in KB {
                c.execute("INSERT INTO kb(title,body,src) VALUES(?1,?2,'bawaan')", (t, b))?;
            }
        }
        if count("pemrograman")? == 0 {
            for (t, b) in crate::kode::KB {
                c.execute("INSERT INTO kb(title,body,src) VALUES(?1,?2,'pemrograman')", (t, b))?;
            }
        }
        Ok(())
    })();
    c.execute_batch(if r.is_ok() { "COMMIT" } else { "ROLLBACK" })?;
    r
}

pub(crate) fn open() -> rusqlite::Result<Connection> {
    static CLEAN: std::sync::Once = std::sync::Once::new();
    let c = Connection::open(config::data_dir().join("pengetahuan.db"))?;
    c.busy_timeout(std::time::Duration::from_secs(10))?;
    c.execute_batch("CREATE VIRTUAL TABLE IF NOT EXISTS kb USING fts5(title, body, src UNINDEXED, tokenize='unicode61');")?;
    seed(&c)?;
    // Sekali per proses: buang duplikat lama (akibat penanaman ganda sebelumnya).
    CLEAN.call_once(|| {
        let _ = c.execute_batch(
            "DELETE FROM kb WHERE rowid NOT IN (SELECT MIN(rowid) FROM kb GROUP BY title, src);
             CREATE TABLE IF NOT EXISTS kb_vec(id INTEGER PRIMARY KEY, v BLOB);
             DELETE FROM kb_vec WHERE id NOT IN (SELECT rowid FROM kb);",
        );
    });
    crate::ingat::init(&c)?;
    Ok(c)
}

pub(crate) fn terms(q: &str) -> Vec<String> {
    const STOP: &[&str] = &["apa", "itu", "yang", "dan", "di", "ke", "dari", "untuk", "adalah", "bagaimana", "jelaskan", "cara", "tanya", "tolong", "dengan", "pada", "ini", "bisa", "kah"];
    q.split(|c: char| !c.is_alphanumeric())
        .map(|w| w.to_lowercase())
        .filter(|w| w.len() > 1 && !STOP.contains(&w.as_str()))
        .take(8)
        .collect()
}

/// Pertanyaan teknik: kalkulator dulu, lalu basis pengetahuan.
pub fn tanya(q: &str) -> String {
    if let Some(a) = calc(q) {
        return a;
    }
    crate::ingat::episode("tanya", q);
    let q = &crate::graf::expand_query(q);
    let hits = crate::ingat::hybrid(q, 3);
    if hits.is_empty() {
        return "Belum ada di basis pengetahuan. Ajari saya lewat misi: pelajari C:/path/buku.pdf".into();
    }
    let mut body = hits
        .iter()
        .map(|d| format!("• {} [{}]: {}", d.title, d.src, d.body.chars().take(450).collect::<String>()))
        .collect::<Vec<_>>()
        .join("\n");
    let rel = crate::ingat::related(&hits[0].title, 5);
    if !rel.is_empty() {
        body.push_str(&format!("\nTerkait: {}", rel.join("; ")));
    }
    if hits.first().is_some_and(|d| d.src == "bawaan") {
        body.push_str(WARN);
    }
    body
}

/// Pelajari dokumen (PDF/txt/md): dipotong dan disimpan ke disk, tidak ditahan di RAM.
pub fn pelajari(path: &str) -> String {
    let text = match crate::alat::read_doc_max(path, 6_000_000) {
        Ok(t) => t,
        Err(e) => return format!("gagal: {e}"),
    };
    let src = std::path::Path::new(path.trim()).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "dokumen".into());
    let n = ingest_max(&src, &text, 20_000);
    format!("Dipelajari: {n} potongan dari '{src}'. Tanyakan lewat misi: tanya <pertanyaan>")
}

/// Potong teks pada batas semantik (paragraf, kalimat, kata) menjadi 400-800 karakter per catatan.
pub fn pecah(text: &str) -> Vec<&str> {
    text_splitter::TextSplitter::new(400..800).chunks(text).collect()
}

/// Potong teks per `pecah` dan simpan ke disk.
/// Mengganti catatan lama dengan label `src` yang sama. Mengembalikan jumlah potongan.
pub fn ingest(src: &str, text: &str) -> usize {
    ingest_max(src, text, 3000)
}

/// Seperti `ingest` dengan batas jumlah potongan sendiri (buku tebal ≈ ribuan potongan).
pub fn ingest_max(src: &str, text: &str, maks: usize) -> usize {
    let Ok(mut c) = open() else { return 0 };
    let Ok(tx) = c.transaction() else { return 0 };
    let _ = tx.execute("DELETE FROM kb WHERE src=?1", [src]);
    let mut n = 0usize;
    for body in pecah(text) {
        let body = body.trim();
        if body.len() > 40 && n < maks {
            let title: String = body.trim_start_matches(|c: char| c == '#' || c.is_whitespace()).lines().next().unwrap_or("").chars().take(50).collect();
            let _ = tx.execute("INSERT INTO kb(title,body,src) VALUES(?1,?2,?3)", (title, body, src));
            n += 1;
        }
    }
    let _ = tx.commit();
    crate::ingat::clear_cache();
    n
}

// ---------- Kalkulator ----------

type Tok = (Option<f64>, String);

fn tokens(q: &str) -> Vec<Tok> {
    q.to_lowercase()
        .replace('²', "2")
        .replace(',', ".")
        .replace(['=', ':', '(', ')'], " ")
        .split_whitespace()
        .map(|t| {
            let end = t.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(t.len());
            match t[..end].parse::<f64>() {
                Ok(v) if end > 0 => (Some(v), t[end..].to_string()),
                _ => (None, t.to_string()),
            }
        })
        .collect()
}

fn get(t: &[Tok], keys: &[&str]) -> Option<f64> {
    for (i, (n, s)) in t.iter().enumerate() {
        if !keys.contains(&s.as_str()) {
            continue;
        }
        let m_of = |u: &str| if matches!(u, "kw" | "kva" | "kvar") { 1000.0 } else { 1.0 };
        let mut mult = m_of(s);
        // "daya 3 kw": satuan setelah angka menentukan pengali.
        if let (Some((Some(_), u1)), Some((None, u))) = (t.get(i + 1), t.get(i + 2)) {
            if u1.is_empty() && keys.contains(&u.as_str()) {
                mult = m_of(u);
            }
        }
        if let Some(v) = n {
            return Some(v * mult);
        }
        if let Some((Some(v), u)) = t.get(i + 1) {
            if u.is_empty() || keys.contains(&u.as_str()) {
                return Some(v * mult);
            }
        }
        if i > 0 {
            if let (Some(v), "") = (t[i - 1].0, t[i - 1].1.as_str()) {
                return Some(v * mult);
            }
        }
    }
    None
}

const STD_MCB: [f64; 12] = [6.0, 10.0, 16.0, 20.0, 25.0, 32.0, 40.0, 50.0, 63.0, 80.0, 100.0, 125.0];
/// KHA indikatif Cu/PVC, 2 penghantar berbeban, metode B1 IEC 60364-5-52 (mm2, A).
const KHA: [(f64, f64); 9] = [(1.5, 17.5), (2.5, 24.0), (4.0, 32.0), (6.0, 41.0), (10.0, 57.0), (16.0, 76.0), (25.0, 101.0), (35.0, 125.0), (50.0, 151.0)];

fn color_digit(w: &str) -> Option<u32> {
    ["hitam", "coklat", "cokelat", "merah", "oranye", "jingga", "kuning", "hijau", "biru", "ungu", "abu", "putih"]
        .iter()
        .position(|c| *c == w)
        .map(|i| match i { 0 => 0, 1 | 2 => 1, 3 => 2, 4 | 5 => 3, 6 => 4, 7 => 5, 8 => 6, 9 => 7, 10 => 8, _ => 9 })
}

fn resistor(q: &str) -> Option<String> {
    let w: Vec<&str> = q.split(|c: char| !c.is_alphabetic()).filter(|x| color_digit(x).is_some() || ["emas", "perak"].contains(x)).collect();
    if w.len() < 3 {
        return None;
    }
    let d = |i: usize| color_digit(w[i]);
    let mult = match w[2] {
        "emas" => 0.1,
        "perak" => 0.01,
        c => 10f64.powi(color_digit(c)? as i32),
    };
    let val = (d(0)? * 10 + d(1)?) as f64 * mult;
    let tol = match w.get(3).copied() {
        Some("emas") => "5%",
        Some("perak") => "10%",
        Some("coklat") | Some("cokelat") => "1%",
        Some("merah") => "2%",
        _ => "20%",
    };
    let fmt = if val >= 1e6 { format!("{:.2} Mohm", val / 1e6) } else if val >= 1e3 { format!("{:.2} kohm", val / 1e3) } else { format!("{val:.1} ohm") };
    Some(format!("Resistor {}: {fmt} ±{tol}", w.join("-")))
}

pub fn calc(q: &str) -> Option<String> {
    let l = q.to_lowercase();
    if l.contains("resistor") || l.contains("warna") {
        if let Some(r) = resistor(&l) {
            return Some(r);
        }
    }
    let t = tokens(q);
    let v = get(&t, &["v", "volt", "tegangan"]);
    let i = get(&t, &["a", "amp", "ampere", "arus", "i"]);
    let r = get(&t, &["r", "ohm", "hambatan", "tahanan"]);
    let p = get(&t, &["w", "watt", "kw", "daya", "p"]);
    let pf = get(&t, &["pf", "cosphi", "cos", "faktor"]).filter(|x| *x > 0.0 && *x <= 1.0);
    let ph3 = ["3 fasa", "tiga fasa", "3fasa", "3 phase", "3ph"].iter().any(|k| l.contains(k));
    let k3 = if ph3 { 3f64.sqrt() } else { 1.0 };
    let pfv = pf.unwrap_or(0.85);
    let has = |ks: &[&str]| ks.iter().any(|k| l.contains(k));

    if has(&["drop tegangan", "susut tegangan", "voltage drop"]) {
        let len = get(&t, &["m", "meter", "panjang"])?;
        let a = get(&t, &["mm2", "luas", "penampang"])?;
        let volt = v.unwrap_or(if ph3 { 380.0 } else { 220.0 });
        let cur = i.or_else(|| p.map(|p| p / (k3 * volt * pfv)))?;
        let rho = if has(&["alumin"]) { 0.028 } else { 0.0175 };
        let dv = if ph3 { k3 * rho * len * cur * pfv / a } else { 2.0 * rho * len * cur / a };
        return Some(format!("Drop tegangan ≈ {dv:.2} V = {:.2}% dari {volt:.0} V (I={cur:.1} A, L={len} m, A={a} mm², rho={rho}).{WARN}", dv / volt * 100.0));
    }
    if has(&["kapasitor", "perbaikan faktor", "perbaikan cos"]) {
        let kw = p? / 1000.0;
        let (p1, p2) = (pf.unwrap_or(0.8), get(&t, &["target"]).filter(|x| *x <= 1.0).unwrap_or(0.95));
        let qc = kw * ((1.0 - p1 * p1).sqrt() / p1 - (1.0 - p2 * p2).sqrt() / p2);
        return Some(format!("Kapasitor ≈ {qc:.2} kVAR untuk {kw} kW dari cosφ {p1} ke {p2}. Qc = P(tanφ1 − tanφ2).{WARN}"));
    }
    if has(&["kabel", "kha", "mcb", "pengaman", "breaker"]) {
        let volt = v.unwrap_or(if ph3 { 380.0 } else { 220.0 });
        let ib = i.or_else(|| p.map(|p| p / (k3 * volt * pfv)))?;
        let mcb = STD_MCB.iter().find(|x| **x >= ib)?;
        let cab = KHA.iter().find(|(_, iz)| *iz >= *mcb).map(|(a, iz)| format!("{a} mm² (KHA indikatif {iz} A)")).unwrap_or("> 50 mm²".into());
        return Some(format!("Ib ≈ {ib:.1} A → MCB {mcb} A → kabel Cu minimal {cab}. Belum termasuk faktor koreksi suhu/kelompok & drop tegangan.{WARN}"));
    }
    if has(&["motor"]) && p.is_some() {
        let volt = v.unwrap_or(380.0);
        let pw = p?;
        let cur = pw / (3f64.sqrt() * volt * 0.85 * 0.85);
        return Some(format!("Motor 3 fasa {pw:.0} W @ {volt:.0} V: I ≈ {cur:.1} A (η 0,85; cosφ 0,85). Arus start DOL ≈ {:.0}-{:.0} A.{WARN}", cur * 5.0, cur * 8.0));
    }
    if has(&["trafo", "transformator", "kva"]) && (p.is_some() || has(&["kva"])) {
        let s = get(&t, &["kva", "w", "daya"])?;
        let volt = v.unwrap_or(380.0);
        return Some(format!("Trafo {s:.0} VA @ {volt:.0} V: I nominal ≈ {:.1} A (3 fasa).", s / (3f64.sqrt() * volt)));
    }
    if has(&["ohm", "hitung", "berapa"]) || [v, i, r, p].iter().flatten().count() >= 2 {
        let (mut v, mut i, mut r, mut p) = (v, i, r, p);
        for _ in 0..2 {
            v = v.or(i.zip(r).map(|(i, r)| i * r)).or(p.zip(i).map(|(p, i)| p / i)).or(p.zip(r).map(|(p, r)| (p * r).sqrt()));
            i = i.or(v.zip(r).map(|(v, r)| v / r)).or(p.zip(v).map(|(p, v)| p / v)).or(p.zip(r).map(|(p, r)| (p / r).sqrt()));
            r = r.or(v.zip(i).map(|(v, i)| v / i)).or(v.zip(p).map(|(v, p)| v * v / p));
            p = p.or(v.zip(i).map(|(v, i)| v * i));
        }
        if [v, i, r, p].iter().flatten().count() >= 3 {
            let f = |x: Option<f64>| x.map(|x| format!("{x:.3}")).unwrap_or("?".into());
            return Some(format!("Hukum Ohm: V={} V, I={} A, R={} Ω, P={} W", f(v), f(i), f(r), f(p)));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ohm_and_resistor() {
        assert!(calc("hitung tegangan 12 arus 2").unwrap().contains("R=6.000"));
        assert!(calc("resistor coklat hitam merah emas").unwrap().contains("1.00 kohm"));
    }
    #[test]
    fn vdrop_and_mcb() {
        let a = calc("drop tegangan panjang 50 m luas 2.5 mm2 arus 13.6 a tegangan 220 v").unwrap();
        assert!(a.contains("Drop tegangan"));
        assert!(calc("pilih kabel dan mcb daya 2000 w 220 v").unwrap().contains("MCB 16"));
    }
}

#[cfg(test)]
mod pecah_tests {
    #[test]
    fn splits_on_boundaries() {
        let t = "Kalimat satu yang cukup panjang untuk diuji. ".repeat(60);
        let v = super::pecah(&t);
        assert!(v.len() > 2 && v.iter().all(|c| c.len() <= 800));
        assert!(v.iter().all(|c| c.trim_end().ends_with('.')));
    }
}

/// Simpan jawaban Gemini/Llama atau hasil pencarian web ke otak (kb + graf) agar bisa dipakai lagi tanpa memanggil model.
/// Hanya jawaban atas pertanyaan (atau hasil web) yang cukup panjang; ditandai belum diverifikasi; maks 400 per sumber.
pub fn simpan_jawaban(kind: &str, goal: &str, ans: &str) {
    let (g, a) = (goal.trim(), ans.trim());
    if a.chars().count() < 120 || g.chars().count() < 8 || a.starts_with("gagal") {
        return;
    }
    let l = g.to_lowercase();
    let tanya = l.contains('?') || ["apa", "siapa", "bagaimana", "kenapa", "mengapa", "kapan", "berapa", "jelaskan", "apakah", "cara", "bedanya", "rumus"].iter().any(|k| l.starts_with(k)) && !l.contains("kabar");
    if kind != "web" && !tanya {
        return;
    }
    let src = format!("belajar-{kind}");
    let judul: String = g.chars().take(80).collect();
    let isi: String = format!("Pertanyaan: {g}\nJawaban ({kind}, belum diverifikasi): {}", a.chars().take(1500).collect::<String>());
    if let Ok(c) = open() {
        let _ = c.execute("DELETE FROM kb WHERE src=?1 AND title=?2", (&src, &judul));
        let _ = c.execute("INSERT INTO kb(title,body,src) VALUES(?1,?2,?3)", (&judul, &isi, &src));
        let _ = c.execute("DELETE FROM kb WHERE src=?1 AND rowid NOT IN (SELECT rowid FROM kb WHERE src=?1 ORDER BY rowid DESC LIMIT 400)", [&src]);
    }
    crate::ingat::clear_cache();
    crate::graf::ingest("jawaban", &format!("{g} {a}"));
}

#[cfg(test)]
mod simpan_tests {
    #[test]
    fn stores_answers_only_when_useful() {
        let panjang = "Kabel NYY 4x16 mm2 memiliki kemampuan hantar arus sekitar 76 ampere pada pemasangan dalam tanah dan cocok untuk feeder panel utama. ".repeat(2);
        super::simpan_jawaban("gemini", "berapa kuat hantar arus kabel nyy 4x16?", &panjang);
        super::simpan_jawaban("gemini", "halo apa kabar kamu hari ini", &panjang); // bukan pertanyaan
        super::simpan_jawaban("web", "x", &panjang); // terlalu pendek
        let c = super::open().unwrap();
        let n: i64 = c.query_row("SELECT COUNT(*) FROM kb WHERE src='belajar-gemini'", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
    }
}

/// Buku/dokumen yang sudah dipelajari (sumber berakhiran .pdf/.md/.txt) beserta jumlah potongan.
pub fn buku_terpelajari() -> Vec<(String, i64)> {
    let Ok(c) = open() else { return vec![] };
    c.prepare("SELECT src, COUNT(*) FROM kb WHERE lower(src) LIKE '%.pdf' OR lower(src) LIKE '%.md' OR lower(src) LIKE '%.txt' GROUP BY src ORDER BY 2 DESC")
        .and_then(|mut st| st.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).map(|r| r.flatten().collect()))
        .unwrap_or_default()
}
