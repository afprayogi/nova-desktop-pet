//! "Resume kita": apa yang NOVA tahu tentang Bos, selera (YouTube/musik/berita), dan hubungan kita. Semua dihitung dari data lokal
//! tanpa model (hemat token). NOVA tidak punya perasaan atau pendapat: "cara NOVA memandang Bos" adalah ringkasan data, dan ditulis begitu.
//! Riwayat browser hanya dibaca bila Bos mengaktifkannya (`/riwayat on`); isinya tidak disimpan.
use std::collections::HashMap;

const GENRE: &[(&str, &[&str])] = &[
    ("anime dan lagu Jepang", &["anime", "opening", "ending", "jpop", "j-pop", "vocaloid", "ghibli", "naruto", "one piece", "jujutsu", "demon slayer"]),
    ("lo-fi dan musik santai", &["lofi", "lo-fi", "chill", "relax", "santai", "piano", "ambient", "acoustic", "instrumental", "study"]),
    ("teknik dan belajar", &["tutorial", "belajar", "plc", "listrik", "motor", "rangkaian", "arduino", "rust", "python", "programming", "engineering", "teknik", "elektro", "simulasi", "kursus", "lecture"]),
    ("musik Indonesia", &["indonesia", "dangdut", "tulus", "hindia", "nadin", "sheila", "jkt48", "pop indo"]),
    ("musik barat dan rock", &["rock", "metal", "hip hop", "rap", "edm", "remix", "coldplay", "billie", "imagine dragons"]),
    ("game", &["gameplay", "minecraft", "genshin", "valorant", "mobile legends", "walkthrough", "game"]),
    ("wawasan dan berita", &["berita", "news", "podcast", "dokumenter", "sejarah", "sains", "explained"]),
    ("hiburan dan komedi", &["lucu", "komedi", "stand up", "meme", "funny", "vlog", "prank"]),
];

/// Judul tontonan YouTube 30 hari terakhir (butuh `/riwayat on`), dengan jumlah kunjungan.
fn tontonan() -> Vec<(String, u32)> {
    if !crate::riwayat::aktif() {
        return vec![];
    }
    let mut m: HashMap<String, u32> = HashMap::new();
    for b in crate::riwayat::baca(30) {
        if !(b.url.contains("youtube.com/watch") || b.url.contains("music.youtube.com")) {
            continue;
        }
        let t = b.judul.trim_end_matches(" - YouTube Music").trim_end_matches(" - YouTube").trim().to_lowercase();
        if !t.is_empty() {
            *m.entry(t).or_insert(0) += b.kunjungan.max(1);
        }
    }
    let mut v: Vec<_> = m.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    v
}

/// Sumber selera: tontonan YouTube, riwayat putar lewat NOVA, dan topik berita pilihan Bos.
fn sumber() -> Vec<(String, u32)> {
    let mut v = tontonan();
    if let Some(o) = std::fs::read_to_string(crate::config::otak_dir().join("lagu.json")).ok().and_then(|j| serde_json::from_str::<serde_json::Value>(&j).ok()).and_then(|j| j["hitung"].as_object().cloned()) {
        v.extend(o.into_iter().map(|(k, n)| (k, n.as_u64().unwrap_or(1) as u32)));
    }
    v.extend(crate::berita::interests().into_iter().map(|t| (t.to_lowercase(), 2)));
    v
}

pub fn selera() -> String {
    let s = sumber();
    if s.is_empty() {
        return "Belum ada data selera. Aktifkan riwayat browser (/riwayat on) agar kubaca tontonan YouTube-mu (hanya judul & jumlah kunjungan, tidak disimpan), atau putar lagu lewat aku, atau atur topik berita (/minat).".into();
    }
    let total: u32 = s.iter().map(|x| x.1).sum();
    let mut skor: Vec<(&str, u32)> = GENRE
        .iter()
        .map(|(g, ks)| (*g, s.iter().filter(|(t, _)| ks.iter().any(|k| t.contains(k))).map(|x| x.1).sum()))
        .filter(|x| x.1 > 0)
        .collect();
    skor.sort_by(|a, b| b.1.cmp(&a.1));
    let mut o = String::from("🎧 Selera Bos (dari tontonan, lagu, dan topik berita):\n");
    for (g, n) in skor.iter().take(4) {
        o.push_str(&format!("• {g} — {}%\n", (*n as f32 * 100.0 / total.max(1) as f32).round()));
    }
    let top: Vec<String> = tontonan().into_iter().take(4).map(|(t, n)| format!("{} ({}x)", crate::research::truncate(&t, 48), n)).collect();
    if !top.is_empty() {
        o.push_str(&format!("Paling sering ditonton: {}\n", top.join("; ")));
    }
    if skor.is_empty() {
        o.push_str("(belum ada yang cocok dengan kategori kenalan; makin banyak data, makin jelas)\n");
    }
    o.push_str("Dihitung lokal dari kata kunci judul; ini perkiraan kasar, bukan penilaian pribadi.");
    o
}

/// Mode kerja dominan 7 hari -> label peran (apa adanya dari data, bukan penilaian).
fn peran() -> Option<(String, String)> {
    let c = crate::assistant::db().ok()?;
    let mut st = c.prepare("SELECT app, SUM(secs) FROM usage WHERE day >= date('now','-7 day') GROUP BY app").ok()?;
    let rows: Vec<(String, i64)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).ok()?.flatten().collect();
    let mut per: HashMap<&'static str, i64> = HashMap::new();
    for (app, secs) in &rows {
        *per.entry(crate::konteks::classify(app).label()).or_insert(0) += secs;
    }
    let total: i64 = per.values().sum();
    let (mode, secs) = per.into_iter().filter(|(m, _)| *m != "Santai").max_by_key(|x| x.1)?;
    let label = match mode {
        "Mode Coding" => "pembuat/pemrogram",
        "Mode Teknik" => "engineer teknik",
        "Mode Dokumen" => "pekerja dokumen",
        "Mode Browsing" => "penjelajah/pencari info",
        "Mode Media" => "penikmat media",
        _ => "pengguna serba-guna",
    };
    Some((label.to_string(), format!("{mode} ≈ {}% waktu aktif 7 hari", secs * 100 / total.max(1))))
}

pub fn resume() -> String {
    let mut o = String::from("🪞 Resume kita\n\n");
    o.push_str("🧑 Yang Bos ceritakan padaku:\n");
    let f = crate::fakta::daftar();
    o.push_str(if f.trim().is_empty() { "(belum ada; ceritakan saja, mis. 'aku suka kopi' atau 'aku bekerja di …')" } else { f.trim() });
    o.push_str("\n\n⏱ Kebiasaan (dari nama aplikasi & jam, tanpa isi):\n");
    for l in crate::assistant::profile_facts() {
        o.push_str(&format!("• {l}\n"));
    }
    o.push('\n');
    o.push_str(&selera());
    o.push_str("\n\n🤝 Hubungan kita:\n");
    let b = crate::buddy::get();
    let hari = (chrono::Local::now().timestamp() - b.lahir).max(0) / 86_400;
    o.push_str(&format!("• Kita sudah bersama ±{hari} hari. {}\n", crate::buddy::bio(&b)));
    o.push_str(&format!("• Gaya yang kupelajari: {}\n", crate::bandit::report().lines().next().unwrap_or("masih belajar")));
    o.push_str("\n🔍 Cara NOVA memandang Bos:\n");
    match peran() {
        Some((p, bukti)) => o.push_str(&format!("• Berdasarkan pemakaian, kucatat Bos sebagai {p} ({bukti}).\n")),
        None => o.push_str("• Belum cukup data pemakaian untuk menyimpulkan.\n"),
    }
    o.push_str("• Aku program, bukan makhluk yang punya perasaan; 'memandang' di sini berarti ringkasan angka di atas, bukan opini atau penilaian.\n");
    o.push_str("\nSemua ini hanya di laptop ini. Hapus yang salah: /lupa <kata>; matikan pembacaan riwayat: /riwayat off.");
    o
}

#[cfg(test)]
mod tests {
    #[test]
    fn genre_cocok_kata_kunci() {
        let t = "lofi hip hop radio - beats to relax/study to";
        assert!(super::GENRE.iter().any(|(g, ks)| *g == "lo-fi dan musik santai" && ks.iter().any(|k| t.contains(k))));
    }
}
