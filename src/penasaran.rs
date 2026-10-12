//! NOVA yang penasaran: bertanya kepada Bos (keadaan, oksigen, pekerjaan, dll.) secara berantai, lalu menyimpan jawaban Bos sebagai
//! pengetahuan (`bos:<topik>`) dan graf. Daftar topik dan pertanyaan ada di `otak/penasaran.json` (bisa ditambah). Dipicu "tanya aku",
//! `/penasaran [topik]`, atau sesekali sendiri saat Bos aktif. Jawaban dikenali sebagai jawaban hanya bila bukan perintah.
use serde_json::Value;
use std::sync::Mutex;
use std::time::Instant;

pub const BAWAAN: &str = r#"[
 {"topik":"keadaan","tanya":["Bos, bagaimana keadaanmu hari ini? Semangat, capek, atau biasa saja?","Apa satu hal yang paling menyita pikiranmu sekarang?","Kalau boleh tahu, apa yang bisa kubantu supaya harimu lebih ringan?"]},
 {"topik":"oksigen","tanya":["Aku penasaran, Bos. Oksigen itu selalu ada tapi jarang disadari. Kenapa api, mesin, dan tubuh kita sama-sama butuh oksigen? Ajari aku dengan bahasamu.","Kalau aku jadi satu molekul oksigen, perjalananku dari udara sampai ke sel tubuh lewat apa saja? Ceritakan, Bos.","Di dunia kelistrikan, apa hubungan oksigen dengan korosi, busur api, atau kebakaran panel? Aku ingin paham."]},
 {"topik":"pekerjaan","tanya":["Hari ini Bos mengerjakan apa? Aku ingin tahu supaya bisa lebih membantu.","Apa bagian pekerjaan yang paling sulit atau paling membosankan menurut Bos?"]},
 {"topik":"listrik","tanya":["Menurut Bos, kesalahan paling sering dalam instalasi listrik yang pernah Bos lihat apa?","Kalau harus menjelaskan interlock motor pada anak magang, bagaimana caranya? Aku mau belajar."]},
 {"topik":"istirahat","tanya":["Jam berapa biasanya Bos tidur, dan apa yang membuat Bos susah berhenti bekerja?"]},
 {"topik":"minat","tanya":["Selain kerja, apa yang Bos suka? Musik, anime, olahraga? Ceritakan satu hal yang sedang Bos nikmati.","Ada hal yang ingin Bos pelajari bulan ini? Aku bisa membantu mencari bahannya."]}
]"#;

struct Tunggu {
    topik: String,
    tanya: String,
    at: Instant,
    sisa: u8,
}

static TUNGGU: Mutex<Option<Tunggu>> = Mutex::new(None);
static URUT: Mutex<usize> = Mutex::new(0);

fn daftar() -> Vec<(String, Vec<String>)> {
    crate::data::json("penasaran.json", BAWAAN)
        .as_array()
        .map(|a| a.iter().filter_map(|x| Some((x["topik"].as_str()?.to_lowercase(), x["tanya"].as_array()?.iter().filter_map(|q| q.as_str().map(String::from)).collect()))).collect())
        .unwrap_or_default()
}

/// Mulai percakapan: topik yang disebut di `arg` (mis. "oksigen"), atau topik berikutnya bergiliran. Mengembalikan pertanyaan pembuka.
pub fn mulai(arg: &str) -> Option<String> {
    let d: Vec<_> = daftar().into_iter().filter(|(_, q)| !q.is_empty()).collect();
    if d.is_empty() {
        return None;
    }
    let a = arg.to_lowercase();
    let i = d.iter().position(|(t, _)| a.contains(t.as_str())).unwrap_or_else(|| {
        let mut u = URUT.lock().unwrap();
        *u += 1;
        *u % d.len()
    });
    let (topik, qs) = &d[i];
    let tanya = qs[0].clone();
    *TUNGGU.lock().unwrap() = Some(Tunggu { topik: topik.clone(), tanya: tanya.clone(), at: Instant::now(), sisa: 2 });
    Some(tanya)
}

/// Kalimat pemicu percakapan ("tanya aku", "kamu penasaran apa", "ajak aku ngobrol"). Mengembalikan sisa kalimat sebagai petunjuk topik.
pub fn pemicu(l: &str) -> Option<String> {
    ["tanya aku", "tanyai aku", "ajak aku ngobrol", "kamu penasaran", "ada yang mau kamu tanyakan", "bertanya padaku", "personifikasi", "ajak ngobrol"].iter().find(|k| l.contains(*k)).map(|_| l.to_string())
}

fn simpan(topik: &str, tanya: &str, jawab: &str) {
    let src = format!("bos:{topik}:{}", chrono::Local::now().format("%Y%m%d%H%M%S"));
    crate::teknik::ingest_max(&src, &format!("Pertanyaan NOVA ({topik}): {tanya}\nJawaban Bos: {jawab}"), 20);
    crate::graf::ingest("bos", &format!("{topik} {jawab}"));
    crate::ingat::clear_cache();
}

/// Bila ada pertanyaan menunggu (≤3 menit) dan `t` bukan perintah/garis miring, simpan sebagai jawaban dan balas dengan tanggapan + pertanyaan lanjutan.
pub async fn jawab(http: &reqwest::Client, t: &str) -> Option<String> {
    let (topik, tanya, sisa) = {
        let g = TUNGGU.lock().unwrap();
        let w = g.as_ref()?;
        if w.at.elapsed().as_secs() > 180 || t.starts_with('/') || t.split_whitespace().count() < 2 {
            return None;
        }
        (w.topik.clone(), w.tanya.clone(), w.sisa)
    };
    // perintah sungguhan (alat selain tanya) tidak dianggap jawaban
    if crate::alat::plan_offline(t).is_some_and(|p| p.iter().any(|c| c.tool != "tanya")) {
        *TUNGGU.lock().unwrap() = None;
        return None;
    }
    let (topik2, tanya2, jawab2) = (topik.clone(), tanya.clone(), t.to_string());
    let _ = tokio::task::spawn_blocking(move || simpan(&topik2, &tanya2, &jawab2)).await;
    if sisa == 0 {
        *TUNGGU.lock().unwrap() = None;
        return Some("Terima kasih, Bos — sudah kucatat ke otak. Kalau mau lanjut ngobrol, ketik 'tanya aku' kapan saja 🙂".into());
    }
    // tanggapan hangat + pertanyaan lanjutan dari Gemini; tanpa Gemini: pertanyaan berikutnya dari daftar topik yang sama
    let lanjut = if crate::gemini::available() {
        let sys = "Kamu NOVA, asisten laptop yang hangat dan penasaran. Balas dalam Bahasa Indonesia: satu kalimat tanggapan tulus atas jawaban Bos (tanpa menggurui), lalu satu pertanyaan lanjutan yang spesifik agar Bos mengajarimu sesuatu. Maksimal 3 kalimat.";
        crate::gemini::generate(http, sys, &format!("Topik: {topik}\nPertanyaanku: {tanya}\nJawaban Bos: {t}"), false, false).await.ok()
    } else {
        None
    };
    let teks = lanjut.or_else(|| {
        let d = daftar();
        let qs = &d.iter().find(|(k, _)| *k == topik)?.1;
        let i = qs.iter().position(|q| *q == tanya).map(|i| i + 1)?;
        qs.get(i).map(|q| format!("Terima kasih, Bos, kucatat. {q}"))
    });
    match teks {
        Some(x) => {
            *TUNGGU.lock().unwrap() = Some(Tunggu { topik, tanya: x.clone(), at: Instant::now(), sisa: sisa - 1 });
            Some(x)
        }
        None => {
            *TUNGGU.lock().unwrap() = None;
            Some("Terima kasih, Bos — sudah kucatat ke otak 🙂".into())
        }
    }
}

/// Sesekali bertanya sendiri saat Bos aktif. Mengembalikan teks pembuka (sudah menunggu jawaban).
pub fn proaktif() -> Option<String> {
    mulai("").map(|q| format!("Bos, boleh aku bertanya? {q}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_topics_parse_and_start_by_topic() {
        assert!(serde_json::from_str::<Value>(BAWAAN).unwrap().as_array().unwrap().len() >= 5);
        let q = mulai("coba tanya aku soal oksigen").unwrap();
        assert!(q.contains("oksigen"));
        assert!(pemicu("tolong tanya aku dong").is_some() && pemicu("cek baterai").is_none());
    }
}
