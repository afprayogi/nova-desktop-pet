//! Persona agen: kartu karakter yang bisa diedit (JSON per agen di folder `persona/`), kepribadian yang BERKEMBANG pelan-pelan
//! dari cara Bos berinteraksi (humor, keformalan, keringkasan, kehangatan), dan impor kartu karakter SillyTavern (V1/V2, JSON/PNG).
//! Teks persona disisipkan ke prompt Gemini dan Llama lokal sehingga gaya bicara konsisten di semua jalur.
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

pub const KEYS: [&str; 4] = ["nova", "volt", "sentry", "memo"];

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Card {
    pub nama: String,
    pub deskripsi: String,
    pub sifat: Vec<String>,
    pub gaya: String,
    pub contoh: Vec<String>,
}

fn dir() -> std::path::PathBuf {
    let d = crate::config::data_dir().join("persona");
    let _ = std::fs::create_dir_all(&d);
    d
}

fn default_card(i: usize) -> Card {
    let p = &crate::agents::PERSONAS[i];
    let (sifat, gaya, contoh): (&[&str], &str, &[&str]) = match i {
        0 => (&["hangat", "sabar", "akrab", "membantu"], "Panggil pengguna 'Bos'. Kalimat pendek dan jelas, boleh satu emoji, tutup dengan tawaran langkah berikutnya.", &["Siap, Bos! Kucek dulu ya 📶", "Tenang Bos, kita beresin satu-satu."]),
        1 => (&["teliti", "teknis", "antusias soal listrik & kode"], "Jelaskan dengan angka dan satuan yang tepat; sebut standar (PUIL/SNI) bila relevan; ringkas.", &["Arus nominalnya I = P / (√3·V·cosφ), Bos."]),
        2 => (&["waspada", "tegas tapi peduli", "jujur soal risiko"], "Peringatkan risiko dengan jelas dan singkat, tanpa menakut-nakuti; beri langkah aman.", &["Awas, disk hampir penuh — ayo bersihkan cache dulu."]),
        _ => (&["perhatian", "tenang", "pengingat yang lembut"], "Hubungkan dengan kebiasaan dan jadwal Bos; ajak istirahat dengan lembut.", &["Sudah 2 jam, Bos. Regangkan bahu sebentar, yuk."]),
    };
    Card { nama: p.name.into(), deskripsi: p.style.into(), sifat: sifat.iter().map(|s| s.to_string()).collect(), gaya: gaya.into(), contoh: contoh.iter().map(|s| s.to_string()).collect() }
}

pub fn card(i: usize) -> Card {
    let i = i.min(3);
    let path = dir().join(format!("{}.json", KEYS[i]));
    if let Some(c) = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str::<Card>(&t).ok()) {
        return c;
    }
    let c = default_card(i);
    let _ = std::fs::write(&path, serde_json::to_string_pretty(&c).unwrap_or_default());
    c
}

// ---------------- kepribadian yang berkembang ----------------

#[derive(Serialize, Deserialize, Clone)]
pub struct Traits {
    pub humor: f32,   // 0 serius .. 1 suka bercanda
    pub santai: f32,  // 0 formal .. 1 santai
    pub ringkas: f32, // 0 detail .. 1 sangat singkat
    pub hangat: f32,  // 0 netral .. 1 sangat hangat
    pub n: u32,
}

impl Default for Traits {
    fn default() -> Self {
        Traits { humor: 0.4, santai: 0.5, ringkas: 0.5, hangat: 0.6, n: 0 }
    }
}

static T: Mutex<Option<Traits>> = Mutex::new(None);

fn tpath() -> std::path::PathBuf {
    dir().join("evolusi.json")
}

pub fn traits() -> Traits {
    let mut g = T.lock().unwrap_or_else(|e| e.into_inner());
    g.get_or_insert_with(|| std::fs::read_to_string(tpath()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()).clone()
}

/// Pelajari gaya Bos dari satu pesan (rata-rata bergerak eksponensial: perubahan pelan, bukan drastis).
pub fn observe(text: &str) {
    let l = text.to_lowercase();
    if l.starts_with('/') || l.len() < 3 {
        return;
    }
    let emoji = text.chars().any(|c| (c as u32) >= 0x1F300);
    let joke = ["wkwk", "haha", "hehe", "lol", "kocak", "lucu", "canda", "wkwkwk"].iter().any(|k| l.contains(k)) || emoji;
    let santai = ["gak", "nggak", "dong", "deh", "sih", "bro", "yuk", "makasih", "aja", "banget"].iter().any(|k| l.split_whitespace().any(|w| w == *k)) || l.len() < 25;
    let formal = ["mohon", "silakan", "terima kasih", "bapak", "ibu", "saudara"].iter().any(|k| l.contains(k));
    let thanks = ["makasih", "terima kasih", "mantap", "bagus", "keren", "thanks"].iter().any(|k| l.contains(k));
    let short = text.split_whitespace().count() <= 5;
    let ema = |v: &mut f32, x: f32| *v = (*v * 0.97 + x * 0.03).clamp(0.05, 0.95);
    let mut t = traits();
    ema(&mut t.humor, joke as u8 as f32);
    ema(&mut t.santai, if formal { 0.0 } else { santai as u8 as f32 });
    ema(&mut t.ringkas, short as u8 as f32);
    if thanks {
        ema(&mut t.hangat, 1.0);
    }
    t.n += 1;
    if let Ok(mut g) = T.lock() {
        *g = Some(t.clone());
    }
    if t.n % 10 == 0 {
        let _ = std::fs::write(tpath(), serde_json::to_string(&t).unwrap_or_default());
    }
}

fn gaya_evolusi(t: &Traits) -> String {
    let mut v = vec![];
    v.push(if t.humor > 0.55 { "sering menyelipkan candaan ringan" } else if t.humor < 0.3 { "serius dan fokus, candaan seperlunya" } else { "candaan sesekali" });
    v.push(if t.santai > 0.6 { "bahasa santai dan akrab" } else if t.santai < 0.35 { "bahasa sopan dan rapi" } else { "bahasa ramah yang wajar" });
    v.push(if t.ringkas > 0.6 { "jawaban sangat ringkas" } else if t.ringkas < 0.35 { "jawaban cukup rinci" } else { "jawaban ringkas" });
    v.push(if t.hangat > 0.7 { "ekspresif dan hangat" } else { "hangat" });
    v.join(", ")
}

/// Versi ringkas untuk model kecil (prompt panjang membuat Llama 1B bingung).
pub fn ringkas(i: usize) -> String {
    let c = card(i);
    let b = if i == 0 { format!(" {}", crate::buddy::prompt()) } else { String::new() };
    format!("Sifatmu: {}. {} Gaya menyesuaikan Bos: {}.{b}", c.sifat.join(", "), c.gaya, gaya_evolusi(&traits()))
}

/// Blok persona untuk prompt agen `i`: kartu + gaya yang berkembang.
pub fn prompt(i: usize) -> String {
    let c = card(i);
    let t = traits();
    let bud = if i == 0 { format!(" {}", crate::buddy::prompt()) } else { String::new() };
    format!(
        "Persona {}: {} Sifat: {}. Aturan gaya: {} Contoh ucapan: {}. Gaya yang menyesuaikan Bos (hasil belajar): {}.{bud}",
        c.nama,
        c.deskripsi,
        c.sifat.join(", "),
        c.gaya,
        c.contoh.iter().map(|x| format!("\"{x}\"")).collect::<Vec<_>>().join(" "),
        gaya_evolusi(&t)
    )
}

// ---------------- impor kartu karakter SillyTavern ----------------

fn png_chara(bytes: &[u8]) -> Option<String> {
    // cari chunk tEXt dengan kata kunci "chara" (isi: base64 JSON)
    let mut i = 8;
    while i + 8 <= bytes.len() {
        let len = u32::from_be_bytes(bytes[i..i + 4].try_into().ok()?) as usize;
        let kind = &bytes[i + 4..i + 8];
        let data = bytes.get(i + 8..i + 8 + len)?;
        if kind == b"tEXt" && data.starts_with(b"chara\0") {
            let b64 = std::str::from_utf8(&data[6..]).ok()?;
            let raw = base64::engine::general_purpose::STANDARD.decode(b64.trim()).ok()?;
            return String::from_utf8(raw).ok();
        }
        i += 12 + len;
    }
    None
}

pub fn from_json(j: &serde_json::Value) -> Option<Card> {
    let d = if j["data"].is_object() { &j["data"] } else { j };
    let s = |k: &str| d[k].as_str().unwrap_or("").trim().to_string();
    let nama = s("name");
    if nama.is_empty() {
        return None;
    }
    let desk = [s("description"), s("scenario")].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" ");
    let sifat: Vec<String> = s("personality").split(|c| c == ',' || c == '\n').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).take(8).collect();
    let contoh: Vec<String> = s("mes_example").split("<START>").map(|x| x.trim().replace("{{char}}", &nama).replace("{{user}}", "Bos")).filter(|x| !x.is_empty()).take(3).map(|x| x.chars().take(240).collect()).collect();
    let ganti = |t: String| t.replace("{{char}}", &nama).replace("{{user}}", "Bos");
    Some(Card { nama: nama.clone(), deskripsi: ganti(desk).chars().take(900).collect(), sifat, gaya: ganti(s("system_prompt")).chars().take(500).collect(), contoh })
}

/// Impor berkas kartu (.json atau .png) ke agen `i`.
pub fn import(path: &str, i: usize) -> Result<String, String> {
    let bytes = std::fs::read(path.trim().trim_matches('"')).map_err(|e| format!("tidak bisa membaca berkas: {e}"))?;
    let text = if bytes.starts_with(&[0x89, b'P', b'N', b'G']) { png_chara(&bytes).ok_or("PNG ini tidak memuat kartu karakter (chunk 'chara')")? } else { String::from_utf8(bytes).map_err(|_| "berkas bukan teks JSON")? };
    let j: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("JSON tidak valid: {e}"))?;
    let c = from_json(&j).ok_or("kartu tidak punya nama (field 'name')")?;
    let i = i.min(3);
    // pertahankan nama agen asli agar sistem tetap konsisten; nama karakter masuk ke deskripsi
    let mut c2 = c.clone();
    c2.nama = crate::agents::PERSONAS[i].name.to_string();
    c2.deskripsi = format!("Berperan dengan karakter '{}': {}", c.nama, c.deskripsi);
    let _ = std::fs::write(dir().join(format!("{}.json", KEYS[i])), serde_json::to_string_pretty(&c2).unwrap_or_default());
    Ok(format!("Kartu karakter '{}' diimpor ke {} ({} sifat, {} contoh).", c.nama, c2.nama, c.sifat.len(), c.contoh.len()))
}

pub fn laporan() -> String {
    let t = traits();
    let mut s = String::from("🎭 Persona agen (berkas di folder data/persona, bisa diedit):\n");
    for i in 0..4 {
        let c = card(i);
        s.push_str(&format!("• {} — {} [{}]\n", c.nama, c.deskripsi.chars().take(80).collect::<String>(), c.sifat.join(", ")));
    }
    s.push_str(&format!("\nKepribadian yang berkembang (dari {} pesan): humor {:.0}% · santai {:.0}% · ringkas {:.0}% · hangat {:.0}%\nGaya sekarang: {}.\nPerintah: /persona impor <berkas.json|png> [nova|volt|sentry|memo] · /persona reset", t.n, t.humor * 100.0, t.santai * 100.0, t.ringkas * 100.0, t.hangat * 100.0, gaya_evolusi(&t)));
    s
}

pub fn reset() {
    if let Ok(mut g) = T.lock() {
        *g = Some(Traits::default());
    }
    let _ = std::fs::remove_file(tpath());
}

pub fn cmd(arg: &str) -> String {
    let mut it = arg.split_whitespace();
    match it.next() {
        Some("impor") | Some("import") => {
            let rest: Vec<&str> = it.collect();
            let (agent, path) = match rest.last().and_then(|l| KEYS.iter().position(|k| k == &l.to_lowercase())) {
                Some(i) if rest.len() > 1 => (i, rest[..rest.len() - 1].join(" ")),
                _ => (0, rest.join(" ")),
            };
            if path.is_empty() {
                return "Pakai: /persona impor <berkas.json|png> [nova|volt|sentry|memo]".into();
            }
            import(&path, agent).unwrap_or_else(|e| format!("Gagal impor: {e}"))
        }
        Some("reset") => {
            reset();
            "Kepribadian yang berkembang dikembalikan ke awal (kartu persona tidak diubah).".into()
        }
        _ => laporan(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn imports_sillytavern_v2() {
        let j = serde_json::json!({"spec": "chara_card_v2", "data": {"name": "Luna", "description": "Asisten ceria {{char}}", "personality": "ceria, lucu", "mes_example": "<START>\n{{user}}: hai\n{{char}}: halo!", "system_prompt": "Bicara santai"}});
        let c = from_json(&j).unwrap();
        assert_eq!(c.nama, "Luna");
        assert!(c.deskripsi.contains("Luna") && c.sifat.contains(&"lucu".to_string()));
        assert!(c.contoh[0].contains("Bos: hai"));
    }
    #[test]
    fn evolves_slowly() {
        let before = traits().humor;
        for _ in 0..20 {
            observe("wkwk kocak banget dong 😂");
        }
        let after = traits().humor;
        assert!(after > before && after - before < 0.5);
    }
}
