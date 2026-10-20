//! Otomasi lanjutan di atas penjadwal: persetujuan tertunda untuk langkah berisiko (tugas berjalan tanpa jendela, jadi izin diminta
//! kemudian lewat NOVA), riwayat jalan, templat siap pakai, dan data/aksi untuk halaman Otomasi di antarmuka.
use crate::alat::{self, Call};
use crate::config::data_dir;
use crate::engine::{Event, Tx};
use crate::keahlian::Langkah;
use crate::penjadwal::{self, Jadwal, Tugas};
use chrono::Local;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;

// ---------- persetujuan tertunda ----------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Menunggu {
    pub id: u32,
    pub sumber: String,
    pub alat: String,
    pub arg: String,
    pub alasan: String,
    pub dibuat: i64,
    #[serde(default)]
    pub diumumkan: bool,
}

fn berkas_menunggu() -> PathBuf {
    data_dir().join("tugas_menunggu.json")
}

/// Daftar menunggu; yang lebih dari 24 jam dianggap basi dan dibuang.
pub fn menunggu() -> Vec<Menunggu> {
    let sekarang = Local::now().timestamp();
    let v: Vec<Menunggu> = std::fs::read_to_string(berkas_menunggu()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    v.into_iter().filter(|m| sekarang - m.dibuat < 24 * 3600).collect()
}

fn tulis_menunggu(v: &[Menunggu]) {
    let _ = std::fs::write(berkas_menunggu(), serde_json::to_string_pretty(v).unwrap_or_default());
}

/// Catat langkah berisiko yang perlu persetujuan Bos. Mengembalikan id antrean.
pub fn tunda(sumber: &str, c: &Call, alasan: &str) -> u32 {
    let mut v = menunggu();
    if let Some(m) = v.iter().find(|m| m.sumber == sumber && m.alat == c.tool && m.arg == c.arg) {
        return m.id; // sudah antre: jangan menumpuk
    }
    let id = v.iter().map(|m| m.id).max().unwrap_or(0) + 1;
    v.push(Menunggu { id, sumber: sumber.into(), alat: c.tool.clone(), arg: c.arg.clone(), alasan: alasan.chars().take(120).collect(), dibuat: Local::now().timestamp(), diumumkan: false });
    tulis_menunggu(&v);
    crate::notify::kirim("NOVA butuh izin", &format!("{sumber}: {} {}\nBuka NOVA untuk menyetujui atau menolak.", c.tool, crate::research::truncate(&c.arg, 80)));
    id
}

/// Umumkan satu permintaan izin sekali waktu lewat kartu pilihan di obrolan (yang berikutnya setelah yang ini dijawab).
pub fn umumkan(tx: &Tx) {
    let mut v = menunggu();
    if v.iter().any(|m| m.diumumkan) {
        return;
    }
    if let Some(m) = v.iter_mut().find(|m| !m.diumumkan) {
        m.diumumkan = true;
        tx.send(Event::Ask {
            question: format!("{} ingin menjalankan: {} {} ({}). Izinkan?", m.sumber, m.alat, crate::research::truncate(&m.arg, 80), m.alasan),
            options: vec![format!("izinkan tugas {}", m.id), format!("tolak tugas {}", m.id)],
        });
        let salin = v.clone();
        tulis_menunggu(&salin);
    }
}

pub async fn izinkan(http: &reqwest::Client, id: u32) -> String {
    let mut v = menunggu();
    let Some(i) = v.iter().position(|m| m.id == id) else { return format!("Tidak ada permintaan izin nomor {id} (mungkin sudah dijawab atau kedaluwarsa).") };
    let m = v.remove(i);
    tulis_menunggu(&v);
    let c = Call { tool: m.alat.clone(), arg: m.arg.clone() };
    let t0 = std::time::Instant::now();
    let r = Box::pin(alat::run(&c, http)).await;
    catat_riwayat(&format!("izin{}", m.id), &m.sumber, "disetujui", t0.elapsed().as_secs(), &format!("{}: {}", m.alat, crate::research::truncate(&r, 400)));
    format!("✅ Diizinkan: {} {}\n{}", m.alat, crate::research::truncate(&m.arg, 80), crate::research::truncate(&r, 800))
}

pub fn tolak(id: u32) -> String {
    let mut v = menunggu();
    let n = v.len();
    v.retain(|m| m.id != id);
    tulis_menunggu(&v);
    if v.len() < n { format!("Permintaan izin {id} ditolak dan dibuang.") } else { format!("Tidak ada permintaan izin nomor {id}.") }
}

// ---------- riwayat ----------

fn berkas_riwayat() -> PathBuf {
    data_dir().join("tugas_hasil.jsonl")
}

pub fn catat_riwayat(kunci: &str, nama: &str, status: &str, detik: u64, hasil: &str) {
    use std::io::Write;
    let baris = json!({"kunci": kunci, "nama": nama, "waktu": Local::now().to_rfc3339(), "status": status, "detik": detik, "hasil": hasil});
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(berkas_riwayat()) {
        let _ = writeln!(f, "{baris}");
    }
}

/// Riwayat terbaru (paling baru dulu); `kunci` memfilter ("t1", "p2", ...).
pub fn riwayat(kunci: Option<&str>, n: usize) -> Vec<Value> {
    let Ok(isi) = std::fs::read_to_string(berkas_riwayat()) else { return vec![] };
    isi.lines().rev().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|v| kunci.is_none_or(|k| v["kunci"].as_str() == Some(k))).take(n).collect()
}

pub fn riwayat_teks(kunci: Option<&str>) -> String {
    let r = riwayat(kunci, 8);
    if r.is_empty() {
        return "Belum ada riwayat jalan.".into();
    }
    let baris = r.iter().map(|v| {
        let w = v["waktu"].as_str().and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok()).map(|d| d.with_timezone(&Local).format("%d/%m %H:%M").to_string()).unwrap_or_default();
        let st = match v["status"].as_str().unwrap_or("ok") { "ok" | "disetujui" => "✔", "gagal" => "✖", _ => "⏳" };
        format!("{w} {st} {} ({} dtk) — {}", v["nama"].as_str().unwrap_or(""), v["detik"].as_u64().unwrap_or(0), crate::research::truncate(v["hasil"].as_str().unwrap_or("").lines().next().unwrap_or(""), 90))
    });
    format!("📜 Riwayat jalan:\n{}", baris.collect::<Vec<_>>().join("\n"))
}

// ---------- templat ----------

pub struct Templat {
    pub nama: &'static str,
    pub ringkas: &'static str,
    pub tipe: &'static str,
    pub jam: &'static str,
    pub hari: &'static [&'static str],
    pub menit: u32,
    pub langkah: &'static [(&'static str, &'static str)],
}

pub const TEMPLAT: &[Templat] = &[
    Templat { nama: "Laporan pagi", ringkas: "Cuaca, agenda, dan kondisi laptop tiap pagi", tipe: "harian", jam: "08:00", hari: &[], menit: 0, langkah: &[("cuaca", ""), ("agenda", ""), ("health", "")] },
    Templat { nama: "Rekap kerja sore", ringkas: "Rekap kerja hari ini di akhir jam kerja", tipe: "mingguan", jam: "16:35", hari: &["senin", "selasa", "rabu", "kamis", "jumat"], menit: 0, langkah: &[("kerja", "rekap")] },
    Templat { nama: "Rekap mingguan", ringkas: "Ringkasan 7 hari tiap Jumat sore", tipe: "mingguan", jam: "16:40", hari: &["jumat"], menit: 0, langkah: &[("kerja", "minggu")] },
    Templat { nama: "Istirahat berkala", ringkas: "Pengingat istirahat tiap 50 menit", tipe: "menit", jam: "", hari: &[], menit: 50, langkah: &[("pesan", "Waktunya istirahat 5 menit: berdiri, minum air, regangkan leher dan punggung.")] },
    Templat { nama: "Bersih mingguan", ringkas: "Tawarkan membersihkan cache aman tiap Jumat (butuh izin Bos)", tipe: "mingguan", jam: "17:00", hari: &["jumat"], menit: 0, langkah: &[("bersih", "")] },
];

pub fn templat_daftar() -> String {
    format!("🧩 Templat jadwal siap pakai (ketik: jadwalkan templat <nomor>):\n{}", TEMPLAT.iter().enumerate().map(|(i, t)| format!("{}. {} — {}", i + 1, t.nama, t.ringkas)).collect::<Vec<_>>().join("\n"))
}

pub fn templat_ke_tugas(i: usize) -> Option<Tugas> {
    let t = TEMPLAT.get(i)?;
    Some(Tugas {
        id: penjadwal::id_baru(),
        nama: t.nama.into(),
        ringkas: t.ringkas.into(),
        jadwal: Jadwal { tipe: t.tipe.into(), jam: t.jam.into(), hari: t.hari.iter().map(|h| h.to_string()).collect(), tanggal: String::new(), tiap_menit: t.menit },
        langkah: t.langkah.iter().map(|(a, g)| Langkah { alat: a.to_string(), arg: g.to_string(), jika: None }).collect(),
        dibuat: Local::now().timestamp(),
    })
}

// ---------- data dan aksi untuk antarmuka ----------

/// Gabungan untuk halaman Otomasi: tugas terjadwal (+status Windows), pemicu, izin menunggu, templat.
pub async fn data_json() -> Value {
    let status = tokio::task::spawn_blocking(penjadwal::status_semua).await.unwrap_or_default();
    let tugas: Vec<Value> = penjadwal::muat()
        .iter()
        .map(|t| {
            let nxt = penjadwal::berikutnya(&t.jadwal, Local::now()).map(|d| d.format("%d/%m %H:%M").to_string());
            let terakhir = riwayat(Some(&format!("t{}", t.id)), 1).into_iter().next();
            json!({"id": t.id, "nama": t.nama, "ringkas": t.ringkas, "jadwal": penjadwal::teks_jadwal(&t.jadwal), "jam": t.jadwal.jam, "tipe": t.jadwal.tipe, "berikutnya": nxt,
                   "status": status.get(&format!("Tugas-{}", t.id)).cloned().unwrap_or_else(|| "Tidak ada di Windows".into()),
                   "langkah": t.langkah.iter().map(|l| format!("{} {}{}", l.alat, l.arg, l.jika.as_ref().map(|j| format!(" [jika {j}]")).unwrap_or_default())).collect::<Vec<_>>(), "terakhir": terakhir})
        })
        .collect();
    json!({
        "tugas": tugas,
        "pemicu": crate::pemicu::daftar_json(),
        "menunggu": menunggu(),
        "templat": TEMPLAT.iter().enumerate().map(|(i, t)| json!({"nomor": i + 1, "nama": t.nama, "ringkas": t.ringkas})).collect::<Vec<_>>(),
        "pemicu_templat": crate::pemicu::TEMPLAT.iter().enumerate().map(|(i, t)| json!({"nomor": i + 1, "nama": t.nama, "ringkas": t.ringkas})).collect::<Vec<_>>(),
        "mulai_otomatis": tokio::task::spawn_blocking(penjadwal::autostart_aktif).await.unwrap_or(false),
    })
}

fn angka(v: &Value, k: &str) -> u32 {
    v[k].as_u64().unwrap_or(0) as u32
}

/// Aksi dari UI. Mengembalikan (berhasil, pesan).
pub async fn aksi(http: &reqwest::Client, v: &Value) -> (bool, String) {
    let id = angka(v, "id");
    let a = v["aksi"].as_str().unwrap_or("");
    let ok = |s: String| (true, s);
    match a {
        "tugas_aktif" | "tugas_nonaktif" => {
            let aktif = a == "tugas_aktif";
            match tokio::task::spawn_blocking(move || penjadwal::ubah_aktif(id, aktif)).await {
                Ok(Ok(())) => ok(if aktif { format!("Tugas {id} diaktifkan.") } else { format!("Tugas {id} dinonaktifkan.") }),
                Ok(Err(e)) => (false, e),
                Err(_) => (false, "gagal".into()),
            }
        }
        "tugas_hapus" => ok(penjadwal::jalankan(http, &format!("hapus {id}")).await),
        "tugas_jalankan" => ok(penjadwal::jalankan(http, &format!("jalankan {id}")).await),
        "tugas_jam" => {
            let jam = v["jam"].as_str().unwrap_or("").to_string();
            match tokio::task::spawn_blocking(move || penjadwal::ubah_jam(id, &jam)).await {
                Ok(Ok(s)) => ok(s),
                Ok(Err(e)) => (false, e),
                Err(_) => (false, "gagal".into()),
            }
        }
        "templat_tugas" => match templat_ke_tugas(angka(v, "nomor").saturating_sub(1) as usize) {
            Some(t) => {
                penjadwal::set_draft(t);
                let r = penjadwal::simpan();
                (r.starts_with('✅'), r)
            }
            None => (false, "Templat tidak ditemukan.".into()),
        },
        "templat_pemicu" => crate::pemicu::buat_dari_templat(angka(v, "nomor").saturating_sub(1) as usize),
        "pemicu_aktif" | "pemicu_nonaktif" => crate::pemicu::ubah_aktif(id, a == "pemicu_aktif"),
        "pemicu_hapus" => ok(crate::pemicu::hapus(id)),
        "izinkan" => ok(izinkan(http, id).await),
        "tolak" => ok(tolak(id)),
        "mulai_otomatis" => {
            let on = v["aktif"].as_bool().unwrap_or(false);
            let r = tokio::task::spawn_blocking(move || penjadwal::autostart_set(on)).await.unwrap_or(false);
            (r, if r { if on { "NOVA akan menyala otomatis saat login.".into() } else { "Nyala otomatis dimatikan.".into() } } else { "Gagal mengubah nyala otomatis.".into() })
        }
        _ => (false, "Aksi tidak dikenal.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn antrean_izin_tidak_menumpuk_dan_bisa_ditolak() {
        let c = Call { tool: "bersih".into(), arg: "x-uji".into() };
        let a = tunda("uji", &c, "alasan");
        let b = tunda("uji", &c, "alasan");
        assert_eq!(a, b, "langkah yang sama tidak boleh antre dua kali");
        assert!(menunggu().iter().any(|m| m.id == a));
        assert!(tolak(a).contains("ditolak"));
        assert!(!menunggu().iter().any(|m| m.id == a));
    }

    #[test]
    fn riwayat_tercatat_dan_terfilter() {
        catat_riwayat("t77", "Uji", "ok", 2, "hasil pertama\nbaris dua");
        catat_riwayat("t78", "Lain", "gagal", 1, "x");
        let r = riwayat(Some("t77"), 5);
        assert_eq!(r.len(), 1);
        assert!(riwayat_teks(Some("t77")).contains("Uji"));
    }

    #[test]
    fn templat_valid_dan_aman() {
        for (i, t) in TEMPLAT.iter().enumerate() {
            let tugas = templat_ke_tugas(i).unwrap();
            assert!(!tugas.langkah.is_empty(), "{}", t.nama);
            for l in &tugas.langkah {
                assert!(crate::registry::ada(&l.alat), "alat '{}' di templat '{}' tidak terdaftar", l.alat, t.nama);
                assert!(!penjadwal::DILARANG.contains(&l.alat.as_str()), "{}", t.nama);
                assert!(!matches!(alat::assess(&Call { tool: l.alat.clone(), arg: l.arg.clone() }), alat::Risk::Block(_)), "{}", t.nama);
            }
            assert!(penjadwal::berikutnya(&tugas.jadwal, Local::now()).is_some(), "{}", t.nama);
        }
    }
}
