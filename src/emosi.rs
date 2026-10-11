//! Analisis emosi OFFLINE untuk teks yang Bos kirim ke bilah obrolan NOVA (bukan ketikan di aplikasi lain).
//! Model ringan berbasis leksikon berbobot (ID+EN) dengan negasi, penguat ("banget"), HURUF BESAR, dan tanda seru.
//! Hasilnya mengubah ekspresi peliharaan dan nada balasan; pilihan gaya balasan dipelajari oleh bandit (bandit.rs).
use serde::Serialize;

#[derive(Clone, Copy, PartialEq, Debug, Serialize)]
pub enum Emosi {
    Netral,
    Stres,
    Lelah,
    Marah,
    Gembira,
    Sedih,
}

impl Emosi {
    pub fn key(&self) -> &'static str {
        match self {
            Emosi::Netral => "netral",
            Emosi::Stres => "stres",
            Emosi::Lelah => "lelah",
            Emosi::Marah => "marah",
            Emosi::Gembira => "gembira",
            Emosi::Sedih => "sedih",
        }
    }
    /// Valensi -1..1 untuk menilai apakah suasana hati membaik.
    pub fn valensi(&self) -> f32 {
        match self {
            Emosi::Gembira => 1.0,
            Emosi::Netral => 0.2,
            Emosi::Lelah => -0.3,
            Emosi::Stres => -0.5,
            Emosi::Sedih => -0.7,
            Emosi::Marah => -0.8,
        }
    }
}

const LEX: &[(Emosi, &[&str])] = &[
    (Emosi::Stres, &["stres", "stress", "pusing", "deadline", "panik", "bingung", "ribet", "overwhelmed", "tertekan", "numpuk", "kewalahan", "buntu", "mumet", "cemas", "khawatir", "takut", "gagal", "error", "bug", "mentok", "pressure", "anxious", "kacau", "berantakan", "gawat"]),
    (Emosi::Lelah, &["capek", "capai", "lelah", "ngantuk", "lemas", "letih", "exhausted", "tired", "sleepy", "begadang", "penat", "burnout", "jenuh", "bosan", "lesu", "kurang tidur", "drained"]),
    (Emosi::Marah, &["kesal", "sebel", "sebal", "marah", "jengkel", "benci", "sialan", "bodoh", "goblok", "muak", "emosi", "angry", "annoying", "damn", "wtf", "brengsek", "kampret", "menyebalkan", "nyebelin", "geram"]),
    (Emosi::Gembira, &["senang", "bahagia", "yey", "hore", "berhasil", "sukses", "mantap", "keren", "asik", "asyik", "makasih", "terima kasih", "thanks", "great", "awesome", "lancar", "selesai", "akhirnya", "yeay", "bangga", "puas", "lega", "love", "wow"]),
    (Emosi::Sedih, &["sedih", "kecewa", "galau", "sepi", "kesepian", "down", "menangis", "nangis", "putus asa", "patah hati", "terpuruk", "hampa", "sad", "lonely", "murung", "menyesal"]),
];

const NEG: &[&str] = &["tidak", "nggak", "gak", "ga", "bukan", "belum", "tak", "ngga", "enggak", "no", "not", "never", "jangan"];
const BOOST: &[&str] = &["banget", "sekali", "sangat", "bgt", "parah", "super", "terlalu", "amat", "really", "very", "so", "benar-benar"];

/// Frasa yang menandakan bahaya bagi diri sendiri: dijawab dengan kepedulian, tanpa bandit/humor.
pub fn krisis(text: &str) -> bool {
    let l = text.to_lowercase();
    ["bunuh diri", "ingin mati", "mau mati", "mengakhiri hidup", "menyakiti diri", "tidak ingin hidup", "pengen mati", "suicide", "kill myself", "end my life"].iter().any(|k| l.contains(k))
}

pub const PESAN_KRISIS: &str = "Aku khawatir mendengarnya, Bos, dan kamu penting. Kalau terasa sangat berat atau ada pikiran menyakiti diri, tolong hubungi orang yang kamu percaya sekarang, atau layanan darurat setempat (di Indonesia: 119 ext. 8 dari Kemenkes). Aku tetap di sini menemanimu.";

/// (emosi dominan, intensitas 0..1). Netral bila sinyal lemah.
pub fn analisis(text: &str) -> (Emosi, f32) {
    let raw: Vec<&str> = text.split(|c: char| c.is_whitespace()).collect();
    let toks: Vec<String> = raw.iter().map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase()).collect();
    let lower = toks.join(" ");
    let mut score = [0f32; 5];
    for (ei, (_, words)) in LEX.iter().enumerate() {
        for w in *words {
            // frasa (mengandung spasi) dicari di teks utuh; kata tunggal per token
            let positions: Vec<usize> = if w.contains(' ') {
                if lower.contains(w) { vec![0] } else { vec![] }
            } else {
                toks.iter().enumerate().filter(|(_, t)| t.as_str() == *w).map(|(i, _)| i).collect()
            };
            for i in positions {
                let mut v = 1.0f32;
                if i > 0 {
                    let lo = i.saturating_sub(2);
                    if toks[lo..i].iter().any(|t| NEG.contains(&t.as_str())) {
                        v = -0.6; // negasi melemahkan/membalik
                    }
                }
                if toks.iter().skip(i.saturating_sub(1)).take(3).any(|t| BOOST.contains(&t.as_str())) {
                    v *= 1.5;
                }
                if raw.get(i).is_some_and(|r| r.len() > 2 && r.chars().filter(|c| c.is_alphabetic()).all(|c| c.is_uppercase())) {
                    v *= 1.3;
                }
                score[ei] += v;
            }
        }
    }
    let bang = text.matches('!').count().min(3) as f32 * 0.15;
    score[3] += if score[3] > 0.0 { bang } else { 0.0 }; // gembira
    score[2] += if score[2] > 0.0 { bang } else { 0.0 }; // marah
    let (mut best, mut bi) = (0.0f32, 0usize);
    for (i, s) in score.iter().enumerate() {
        if *s > best {
            best = *s;
            bi = i;
        }
    }
    if best < 1.0 {
        return (Emosi::Netral, 0.0);
    }
    (LEX[bi].0, (best / 3.0).min(1.0))
}

/// Balasan menurut emosi dan gaya (empati | semangat | humor). Indeks varian dari `seed`.
pub fn balasan(e: Emosi, gaya: &str, seed: usize) -> Option<String> {
    let set: &[&str] = match (e, gaya) {
        (Emosi::Stres, "empati") => &["Kedengarannya berat ya, Bos. Tarik napas 4 detik, tahan 4, buang 6. Satu masalah dulu, yang lain nanti.", "Aku di sini. Mau kupecah tugasnya jadi langkah kecil? Ketik: ingatkan 25 menit lagi istirahat."],
        (Emosi::Stres, "semangat") => &["Bos pernah melewati yang lebih sulit. Selesaikan satu hal kecil dulu, momentum akan menyusul!", "Masalah yang rumit itu cuma kumpulan masalah kecil. Ayo kita urai satu per satu."],
        (Emosi::Stres, "humor") => &["Kata orang bug itu fitur yang belum diumumkan. Kita umumkan dulu atau kita tangkap? 😄", "Tenang, komputer juga tidak tahu apa yang dia lakukan. Kita sama-sama belajar."],
        (Emosi::Lelah, "empati") => &["Bos kelihatan capek. Istirahat 5 menit: berdiri, regangkan badan, minum air putih. Aku jaga di sini.", "Tubuh butuh jeda, itu wajar. Mau kubuatkan pengingat istirahat 10 menit lagi?"],
        (Emosi::Lelah, "semangat") => &["Sedikit lagi, Bos! Setelah ini kita istirahat yang pantas.", "Kerja kerasmu terlihat. Pelan tidak apa-apa, yang penting jalan."],
        (Emosi::Marah, "empati") => &["Wajar kalau kesal, Bos. Jeda sebentar dulu: tarik napas dalam, lalu kita lihat masalahnya bersama.", "Aku mengerti itu menyebalkan. Mau cerita atau mau langsung kucarikan solusinya?"],
        (Emosi::Gembira, "semangat") => &["Yeay, mantap, Bos! Rayakan dulu sebentar 🎉", "Kerja bagus! Senang melihat Bos begini."],
        (Emosi::Gembira, "humor") => &["Wah, jangan-jangan kode-nya sekali jalan langsung benar? Itu langka seperti komet! ☄️", "Hore! Aku ikut joget pixel dulu ya 🕺"],
        (Emosi::Sedih, "empati") => &["Aku turut merasakannya, Bos. Tidak apa-apa untuk merasa begini. Aku ada di sini kalau mau bercerita.", "Pelan-pelan saja. Kalau berkenan, ceritakan apa yang terjadi, atau kita jeda sebentar."],
        (Emosi::Sedih, "semangat") => &["Hari yang berat akan lewat, Bos. Satu langkah kecil hari ini sudah cukup.", "Kamu lebih kuat dari yang kamu kira. Aku percaya padamu."],
        _ => return None,
    };
    Some(set[seed % set.len()].to_string())
}

/// Gaya yang diizinkan untuk tiap emosi (humor tidak untuk marah/sedih).
pub fn gaya_diizinkan(e: Emosi) -> &'static [&'static str] {
    match e {
        Emosi::Stres => &["empati", "semangat", "humor", "diam"],
        Emosi::Lelah => &["empati", "semangat", "diam"],
        Emosi::Marah => &["empati", "diam"],
        Emosi::Sedih => &["empati", "semangat", "diam"],
        Emosi::Gembira => &["semangat", "humor", "diam"],
        Emosi::Netral => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detects_emotions() {
        assert_eq!(analisis("aku stres banget deadline numpuk").0, Emosi::Stres);
        assert_eq!(analisis("capek banget, ngantuk berat").0, Emosi::Lelah);
        assert_eq!(analisis("KESAL banget sama error ini!!").0, Emosi::Marah);
        assert_eq!(analisis("akhirnya berhasil, mantap!").0, Emosi::Gembira);
        assert_eq!(analisis("hari ini aku sedih dan kecewa").0, Emosi::Sedih);
        assert_eq!(analisis("kapan baterai habis").0, Emosi::Netral);
        assert_eq!(analisis("aku tidak capek kok").0, Emosi::Netral);
        assert!(krisis("aku ingin mati") && !krisis("aku capek"));
        assert!(balasan(Emosi::Marah, "humor", 0).is_none());
    }
}

