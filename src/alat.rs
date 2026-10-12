//! Alat kontrol komputer untuk para agen. Tindakan berisiko selalu butuh persetujuan Bos.
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::process::Command;

/// True = akses komputer dikunci (mis. web terbuka ke jaringan tanpa password).
pub static LOCKED: AtomicBool = AtomicBool::new(false);

/// True bila rencana terakhir dari plan_offline hanya hasil tebakan ML (bukan aturan pasti): Gemini lebih diutamakan.
pub static FROM_ML: AtomicBool = AtomicBool::new(false);

pub const TOOL_HELP: &str = "\
kurikulum <buku|topik> [N hari|minggu] [N menit|jam]: jadwal belajar dari buku + pengingat
berita [topik]: berita terbaru sesuai minat (butuh internet)
kepribadian   : preferensi yang sudah kupelajari tentang Bos
cari <apa>    : cari file di komputer (nama, jenis, waktu, atau 'berisi kata')
rapikan <folder>: pratinjau merapikan folder per jenis file (dijalankan setelah Bos membalas ya)
apps          : pratinjau aplikasi berat yang tidak dipakai (ditutup sopan setelah Bos membalas ya)
urungkan      : kembalikan file hasil rapikan terakhir
meme [topik]  : hibur dengan meme
bukapath <jalur>: buka berkas/folder yang baru Bos seret/pilih (hanya jalur itu)
jadwal        : rencana tidur — apakah masih bisa tidur 8 jam dengan jadwal/pengingat/acara yang ada
otomatis <kecerahan|suara> <on|off>: atur kecerahan/suara otomatis menurut jam & kebiasaan
wifi          : Wi-Fi tersambung + jaringan di sekitar
port          : port yang sedang listening beserta prosesnya
bukafile <nama>: cari berkas lalu BUKA langsung (dokumen/gambar/media; yang bisa dieksekusi hanya ditunjukkan foldernya)
cli <alat> [sub]: bantuan perintah CLI (git, cargo, docker, ollama, npm, python, winget, ping, dll.; hanya --help)
atur <kecerahan|volume> <N|+N|-N|mute|unmute|?>: atur kecerahan layar / volume suara
waktu         : jam, hari, tanggal sekarang
web <tanya>   : cari informasi terbaru di internet
harga <aset>: harga + grafik 30 hari (saham bbca/AAPL, kurs dolar, emas, bitcoin, ihsg)
pantau <aset> di atas|di bawah <angka>: kabari saat harga melewati ambang; cuacapantau <kota>: pengingat hujan/panas
gitringkas [folder]: ringkasan repo git (status, commit terakhir, diff) + saran pesan commit
halaman <url>: ringkas isi halaman web; csv <jalur.csv>: analisis cepat berkas CSV
hitung <ekspresi>: kalkulator (+ - * / ^ %, sqrt, sin, cos, ln, pi), mis. hitung 12*(3+4)^2
konversi <angka> <satuan> ke <satuan>: panjang, massa, daya, tekanan, energi, suhu, dll.
zona <kota/jam>: waktu di kota lain atau konversi jam, mis. jam 14:00 wib di london
ohm <V= I= R= P=>: hukum Ohm dari dua nilai; fase3 <P= V= cos=>: arus 3 fase
hari <tanggal>: berapa hari lagi/yang lalu; password [panjang]: kata sandi acak
catat <teks> | catatan [kata|hapus N]: catatan cepat
cuaca [kota]: cuaca & prakiraan 3 hari; kurs <jumlah> <MATA> ke <MATA>: nilai tukar
wiki <topik>: ringkasan Wikipedia; berpikir <masalah>: penalaran langkah demi langkah; terjemah <teks> ke <bahasa>
jurnal <topik [tahun]>: cari jurnal/makalah ilmiah (OpenAlex), mis. semantic 2026 kurang 5 tahun
gambar <kata kunci>: cari gambar di internet (thumbnail + tautan sumber)
bersih        : bersihkan cache aman (Temp, npm, pip, Hugging Face) — butuh persetujuan
kode <nama folder>: buka folder proyek di VS Code (dicari otomatis di D:/repo, Documents, dll.)
winapp <nama> : buka aplikasi bawaan Windows (kalkulator, notepad, paint, pengaturan wifi, dll.)
tugas <teks>   : set/lihat tugas utama hari ini ('selesai' untuk menandai)
musik         : putar musik fokus (folder Musik atau pencarian browser)
bukahasil <n> : buka folder hasil pencarian ke-n di Explorer
ingatan <topik>: apa yang pernah kita bahas tentang topik (graf ingatan jangka panjang)
graphify <folder>: petakan folder kode/dokumen ke graf ingatan
email         : ringkasan email belum dibaca di semua akun (Google + IMAP)
baterai       : sisa baterai + prediksi habis/penuh (model belajar per konteks)
ngobrol <teks>: sapaan/obrolan ringan
tanya <teks>  : pertanyaan/hitungan teknik listrik, elektronika, PLC (kalkulator + basis pengetahuan)
pelajari <path>: serap PDF/txt/md ke basis pengetahuan (untuk dijawab lewat tanya)
remind <teks> : pengingat, mis. 'jam 15:30 rapat' atau '20 menit lagi minum'
agenda / habit / tidur : agenda hari ini / kebiasaan / laporan tidur
layar         : baca isi layar dengan OCR Tesseract lokal (butuh izin Bos); arg 'ringkas' untuk ringkasan
pdf <path>    : ambil teks PDF/txt/md (untuk diringkas)
ringkas <path>: ringkasan ekstraktif PDF/txt/md tanpa model
health        : kondisi CPU/RAM/disk dan proses terberat
info          : info OS, folder kerja, waktu
ls <path>     : isi folder
read <path>   : baca file teks
ps            : daftar proses
fetch <url>   : ambil teks halaman web
shell <cmd>   : jalankan perintah shell (butuh persetujuan Bos)
write <path>  : tulis file; baris pertama arg = path, sisanya isi (butuh persetujuan Bos)
open <target> : buka URL/file/aplikasi (butuh persetujuan Bos)cariisi <berkas> :: <pertanyaan>: potongan paling relevan dari isi sebuah berkas (txt/md/csv/json/pdf/docx)
github <kata>   : cari repositori GitHub (bintang, bahasa, lisensi)
tunggu <detik>  : jeda 1-30 detik di antara langkah
kompres <folder|berkas>: buat zip di sebelahnya (butuh persetujuan)
";

#[derive(Clone, Serialize, Deserialize)]
pub struct Call {
    pub tool: String,
    pub arg: String,
}

pub enum Risk {
    Safe,
    Confirm(String),
    Block(String),
}

const DENY: &[&str] = &[
    "rm -rf /", "rm -rf ~", "rm -rf *", "rm -fr /", "mkfs", "format c:", "format d:", "del /s", "rd /s /q c:", "rmdir /s /q c:",
    "shutdown", "reboot", "reg delete", "diskpart", ":(){", "dd if=", "of=/dev/sd", "cipher /w", "bcdedit", "net user",
    "| sh", "| bash", "invoke-expression", "iex ", "downloadstring", "remove-item -recurse -force c:", "chmod -r 777 /",
];
const SENSITIVE: &[&str] = &[".ssh", "id_rsa", ".aws", ".env", "passwd", "shadow", "credentials", "config.json", "cookies", "login data"];

/// Perintah shell yang hanya membaca (tanpa pengalihan/penyambungan perintah) tidak perlu persetujuan.
fn shell_readonly(a: &str) -> bool {
    if a.chars().any(|c| ">&;`
".contains(c)) || a.contains("$(") {
        return false;
    }
    let a = a.trim();
    let first = a.split_whitespace().next().unwrap_or("");
    if ["dir", "ls", "type", "cat", "echo", "whoami", "hostname", "ipconfig", "ping", "tasklist", "systeminfo", "where", "ver", "date", "tree", "findstr", "get-childitem", "get-content", "get-process", "get-date", "netstat", "nslookup", "tracert"].contains(&first) {
        return true;
    }
    first == "git" && ["status", "log", "diff", "branch", "show", "remote", "--version"].contains(&a.split_whitespace().nth(1).unwrap_or(""))
}

pub fn assess(c: &Call) -> Risk {
    let arg = c.arg.to_lowercase();
    if LOCKED.load(Ordering::Relaxed) && c.tool != "info" {
        return Risk::Block("akses komputer dikunci: server web terbuka tanpa KH_PASSWORD".into());
    }
    match c.tool.as_str() {
        "info" | "ps" | "health" | "kode" | "gambar" | "jurnal" | "harga" | "pantau" | "cuacapantau" | "gitringkas" | "halaman" | "peramban" | "internet" | "riwayat" | "csv" | "hitung" | "konversi" | "zona" | "ohm" | "fase3" | "hari" | "password" | "catat" | "catatan" | "cuaca" | "kurs" | "wiki" | "berpikir" | "terjemah" | "jadwal" | "otomatis" | "wifi" | "port" | "bukafile" | "cli" | "atur" | "winapp" | "waktu" | "web" | "tugas" | "musik" | "bukahasil" | "ingatan" | "email" | "cari" | "rapikan" | "apps" | "meme" | "baterai" | "ngobrol" | "berita" | "kepribadian" | "remind" | "agenda" | "habit" | "tidur" | "tanya" => Risk::Safe,
        "kurikulum" if SENSITIVE.iter().any(|s| arg.contains(s)) => Risk::Confirm("path tampak sensitif".into()),
        "graphify" if SENSITIVE.iter().any(|s| arg.contains(s)) => Risk::Confirm("path tampak sensitif".into()),
        "graphify" => Risk::Safe,
        "pelajari" if SENSITIVE.iter().any(|s| arg.contains(s)) => Risk::Confirm("path tampak sensitif".into()),
        "pelajari" | "kurikulum" => Risk::Safe,
        "kirim" => Risk::Confirm("mengirim email ke alamat Bos sendiri".into()),
        "otak" => Risk::Safe,
        "cariisi" if SENSITIVE.iter().any(|x| arg.contains(x)) => Risk::Confirm("path tampak sensitif".into()),
        "cariisi" | "github" | "tunggu" => Risk::Safe,
        "kompres" if SENSITIVE.iter().any(|x| arg.contains(x)) => Risk::Block("path sensitif tidak dikompres".into()),
        "kompres" => Risk::Confirm(format!("membuat berkas zip dari {}", c.arg.trim())),
        "paket" if arg.contains("tangkap") => Risk::Confirm("menangkap paket jaringan ±15 detik (Windows akan meminta UAC)".into()),
        "paket" => Risk::Safe,
        "daya" if matches!(arg.trim(), "kunci" | "batal") => Risk::Safe,
        "daya" => Risk::Confirm(format!("daya laptop: {}", arg.trim())),
        "urungkan" => Risk::Confirm("mengembalikan perubahan berkas terakhir (tulis/rapikan)".into()),
        "layar" => Risk::Safe, // hanya membaca (OCR lokal, tidak disimpan)
        "pdf" | "ringkas" if SENSITIVE.iter().any(|s| arg.contains(s)) => Risk::Confirm("path tampak sensitif".into()),
        "pdf" | "ringkas" => Risk::Safe,
        "fetch" if arg.starts_with("http://") || arg.starts_with("https://") => Risk::Safe,
        "fetch" => Risk::Block("fetch hanya menerima URL http(s)".into()),
        "ls" | "read" => {
            if SENSITIVE.iter().any(|s| arg.contains(s)) {
                Risk::Confirm("path tampak sensitif (kunci/rahasia/konfigurasi)".into())
            } else {
                Risk::Safe
            }
        }
        "shell" => match DENY.iter().find(|d| arg.contains(*d)) {
            Some(d) => Risk::Block(format!("perintah berbahaya terdeteksi: '{d}'")),
            None if SENSITIVE.iter().any(|s| arg.contains(s)) => Risk::Confirm("perintah menyentuh jalur sensitif (kunci/rahasia/konfigurasi)".into()),
            None if shell_readonly(&arg) => Risk::Safe,
            None => Risk::Confirm("menjalankan perintah yang bisa mengubah komputer Bos (edit/tambah/hapus)".into()),
        },
        "bersih" => Risk::Confirm("menghapus cache yang bisa diunduh ulang (Temp lama, npm, pip, Hugging Face)".into()),
        "write" => Risk::Confirm("menulis/menimpa file di komputer Bos".into()),
        "open" => {
            let url = c.arg.starts_with("https://") || c.arg.starts_with("http://");
            if c.arg.chars().any(|ch| if url { "&|<>^\"'\n ".contains(ch) } else { "&|<>^\"%'\n".contains(ch) }) {
                Risk::Block("target open memuat karakter terlarang".into())
            } else {
                Risk::Safe // membuka tidak mengubah data; karakter berbahaya sudah diblokir di atas
            }
        }
        t => Risk::Block(format!("alat tidak dikenal: {t}")),
    }
}

fn cut(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…[dipotong]", s.chars().take(n).collect::<String>())
    }
}

async fn sh(cmd: &str) -> String {
    let mut c = if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.args(["/C", cmd]);
        c
    } else {
        let mut c = Command::new("sh");
        c.args(["-c", cmd]);
        c
    };
    c.kill_on_drop(true).stdin(std::process::Stdio::null());
    match tokio::time::timeout(Duration::from_secs(30), c.output()).await {
        Err(_) => "timeout 30 detik, proses dihentikan".into(),
        Ok(Err(e)) => format!("gagal menjalankan: {e}"),
        Ok(Ok(o)) => {
            let mut t = String::from_utf8_lossy(&o.stdout).to_string();
            let e = String::from_utf8_lossy(&o.stderr);
            if !e.trim().is_empty() {
                t.push_str("\n[stderr] ");
                t.push_str(&e);
            }
            format!("[exit {}] {}", o.status.code().unwrap_or(-1), t.trim())
        }
    }
}

fn strip_tags(html: &str) -> String {
    let (mut out, mut tag) = (String::new(), false);
    for ch in html.chars() {
        match ch {
            '<' => tag = true,
            '>' => {
                tag = false;
                out.push(' ');
            }
            c if !tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Jalankan alat (risiko sudah dinilai/disetujui oleh pemanggil).
pub async fn run(c: &Call, http: &reqwest::Client) -> String {
    let arg = c.arg.trim();
    let out = match c.tool.as_str() {
        "info" => format!(
            "os={} arch={} cwd={} user={} waktu_unix={}",
            std::env::consts::OS,
            std::env::consts::ARCH,
            std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_default(),
            std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_default(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
        ),
        "health" => crate::monitor::summary(),
        "pdf" | "ringkas" => {
            let p = arg.to_string();
            match tokio::task::spawn_blocking(move || read_doc(&p)).await {
                Ok(Ok(t)) if c.tool == "ringkas" => summarize(&t, 6),
                Ok(Ok(t)) => t.chars().take(6000).collect(),
                Ok(Err(e)) => format!("gagal: {e}"),
                Err(_) => "gagal: dokumen tidak bisa dibaca".into(),
            }
        }
        "layar" => match tokio::task::spawn_blocking(crate::layar::read_screen).await {
            Ok(Ok(t)) if t.trim().is_empty() => "layar tidak berisi teks yang terbaca".into(),
            Ok(Ok(t)) if arg.contains("ringkas") => summarize(&t, 5),
            Ok(Ok(t)) => t.chars().take(1500).collect(),
            Ok(Err(e)) => format!("gagal: {e}"),
            Err(_) => "gagal membaca layar".into(),
        },
        "baterai" => crate::baterai::report(),
        "email" => crate::google::summary(http).await,
        "ingatan" => {
            let a = arg.to_string();
            tokio::task::spawn_blocking(move || crate::graf::about(&a)).await.unwrap_or_else(|_| "gagal membaca graf".into())
        }
        "graphify" => {
            let a = arg.trim().trim_matches('"').to_string();
            tokio::task::spawn_blocking(move || {
                let p = std::path::Path::new(&a);
                if p.is_dir() { crate::graf::graphify_folder(p) } else { format!("Folder tidak ditemukan: {a}") }
            })
            .await
            .unwrap_or_else(|_| "gagal memetakan folder".into())
        }
        "cari" => {
            let a = arg.to_string();
            let r = tokio::task::spawn_blocking(move || {
                let r = crate::berkas::cari(&a);
                if r.starts_with("🔎 Ketemu") {
                    crate::indeks::reveal(1); // langsung tunjukkan foldernya
                }
                r
            })
            .await
            .unwrap_or_else(|_| "pencarian gagal".into());
            r
        }
        "atur" => {
            let a = arg.to_string();
            tokio::task::spawn_blocking(move || crate::sistem::atur(&a)).await.unwrap_or_else(|_| "gagal mengatur".into())
        }
        "waktu" => crate::sistem::waktu_text(),
        "bukapath" => {
            let a = arg.to_string();
            tokio::task::spawn_blocking(move || if crate::seret::is_dropped(&a) { crate::seret::open_path(&a) } else { "Jalur itu bukan berkas yang baru Bos seret/pilih, jadi tidak kubuka.".into() }).await.unwrap_or_else(|_| "gagal membuka".into())
        }
        "jadwal" if arg.contains("sekarang") => tokio::task::spawn_blocking(crate::rencana::jika_tidur_sekarang).await.unwrap_or_else(|_| "gagal menghitung".into()),
        "jadwal" => tokio::task::spawn_blocking(crate::rencana::plan).await.unwrap_or_else(|_| "gagal menghitung rencana tidur".into()),
        "otomatis" => crate::sistem::auto_cmd(arg),
        "wifi" => tokio::task::spawn_blocking(crate::sistem::wifi).await.unwrap_or_else(|_| "gagal membaca Wi-Fi".into()),
        "port" => tokio::task::spawn_blocking(crate::makro::ports).await.unwrap_or_else(|_| "gagal membaca port".into()),
        "bukafile" => {
            let a = arg.to_string();
            tokio::task::spawn_blocking(move || {
                let r = crate::berkas::cari(&a);
                if r.starts_with("🔎 Ketemu") {
                    format!("{}\n{}", crate::indeks::open_file(1), r.lines().skip(1).take(3).collect::<Vec<_>>().join("\n"))
                } else {
                    r
                }
            })
            .await
            .unwrap_or_else(|_| "pencarian gagal".into())
        }
        "cli" => {
            let a = arg.to_string();
            tokio::task::spawn_blocking(move || crate::cli::help(&a)).await.unwrap_or_else(|_| "gagal membaca bantuan".into())
        }
        "web" => crate::gemini::web(http, &crate::rawat::perluas(arg)).await,
        "gambar" => crate::gambar::cari(http, arg).await,
        "harga" => crate::harga::harga(http, arg).await,
        "pantau" => crate::harga::pantau(http, arg).await,
        "cuacapantau" => crate::harga::cuaca_pantau(arg),
        "gitringkas" => crate::ekstra::git_ringkas(http, arg).await,
        "halaman" => crate::ekstra::halaman(http, arg).await,
        "peramban" => crate::peramban::baca(arg).await,
        "internet" => crate::internet::laporan(http).await,
        "riwayat" => tokio::task::spawn_blocking(crate::riwayat::ringkas).await.unwrap_or_default(),
        "daya" => crate::daya::jalankan(arg),
        "cariisi" => {
            let a = arg.to_string();
            tokio::task::spawn_blocking(move || crate::alatbaru::cari_isi(&a)).await.unwrap_or_default()
        }
        "github" => crate::alatbaru::github(http, arg).await,
        "tunggu" => crate::alatbaru::tunggu(arg).await,
        "kompres" => {
            let a = arg.to_string();
            tokio::task::spawn_blocking(move || crate::alatbaru::kompres(&a)).await.unwrap_or_default()
        }
        "otak" => tokio::task::spawn_blocking(crate::otak::teks).await.unwrap_or_default(),
        "paket" if arg.contains("tangkap") => crate::paket::tangkap(15).await,
        "paket" => crate::paket::teks_monitor(&crate::paket::monitor().await),
        "kirim" => crate::surel::kirim(arg).await,
        "csv" => crate::ekstra::csv(arg),
        "hitung" => crate::perkakas::hitung(arg),
        "konversi" => crate::perkakas::konversi(arg),
        "zona" => crate::perkakas::zona(arg),
        "ohm" => crate::perkakas::ohm(arg),
        "fase3" => crate::perkakas::fase3(arg),
        "hari" => crate::perkakas::hari(arg),
        "password" => crate::perkakas::password(arg),
        "catat" => crate::perkakas::catat(arg),
        "catatan" => crate::perkakas::catatan(arg),
        "cuaca" => crate::daring::cuaca(http, arg).await,
        "kurs" => crate::daring::kurs(http, arg).await,
        "wiki" => crate::daring::wiki(http, arg).await,
        "berpikir" => crate::daring::berpikir(http, arg).await,
        "terjemah" => crate::daring::terjemah(http, arg).await,
        "jurnal" => crate::ilmiah::cari(http, arg).await,
        "bersih" => tokio::task::spawn_blocking(crate::sistem::bersih_aman).await.unwrap_or_else(|_| "gagal membersihkan".into()),
        "kode" => {
            let a = arg.to_string();
            tokio::task::spawn_blocking(move || crate::sistem::kode(&a)).await.unwrap_or_else(|_| "gagal membuka VS Code".into())
        }
        "winapp" => {
            let a = arg.to_string();
            tokio::task::spawn_blocking(move || crate::sistem::winapp(&a)).await.unwrap_or_else(|_| "gagal membuka aplikasi".into())
        }
        "tugas" => crate::tugas::set(arg),
        "musik" => tokio::task::spawn_blocking(crate::musik::play).await.unwrap_or_else(|_| "gagal memutar musik".into()),
        "bukahasil" => crate::indeks::reveal(arg.chars().filter(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(1)),
        "rapikan" => {
            let a = arg.to_string();
            tokio::task::spawn_blocking(move || crate::berkas::rapikan(&a)).await.unwrap_or_else(|_| "gagal menyusun rencana".into())
        }
        "apps" => tokio::task::spawn_blocking(crate::berkas::apps).await.unwrap_or_else(|_| "gagal memeriksa aplikasi".into()),
        "urungkan" => {
            // urungkan aksi TERBARU: penulisan berkas (jurnal) atau rapikan, mana yang lebih baru
            if crate::jurnal::latest_ts() > crate::berkas::undo_mtime() {
                crate::jurnal::undo_last().unwrap_or_else(|| "Tidak ada yang bisa diurungkan.".into())
            } else {
                crate::berkas::urungkan()
            }
        }
        "meme" => crate::meme::meme(arg, arg.len() + chrono::Timelike::second(&chrono::Local::now()) as usize),
        "berita" => crate::berita::headlines(http, arg, false).await.0,
        "kepribadian" => crate::bandit::report(),
        "kurikulum" => {
            let a = arg.to_string();
            tokio::task::spawn_blocking(move || crate::kurikulum::run(&a)).await.unwrap_or_else(|_| "gagal membuat kurikulum".into())
        }
        "ngobrol" => ngobrol(arg),
        "remind" => crate::assistant::add_reminder(arg),
        "agenda" | "habit" | "tidur" => crate::assistant::report(&c.tool),
        "tanya" => {
            let nv = crate::vektor::tanya(http, arg).await;
            let mut r = crate::teknik::tanya_v(arg, nv.as_deref());
            // Buku berbahasa Inggris: pertanyaan Indonesia juga dicari dalam bentuk Inggris (pencarian berbasis kata tidak lintas bahasa).
            if arg.split_whitespace().count() >= 3 && !crate::terjemah::kemungkinan_inggris(arg) && !crate::teknik::buku_terpelajari().is_empty() {
                if let Ok((en, _, _)) = crate::terjemah::terjemahkan(http, arg, Some("en")).await {
                    let r2 = crate::teknik::tanya(&en);
                    if r2.contains(".pdf]") || r2.contains(".md]") || r2.contains(".txt]") {
                        r = r2;
                    }
                }
            }
            // Sumber berbahasa Inggris (mis. buku teknik): sertakan terjemahan Indonesia di depan.
            if crate::terjemah::kemungkinan_inggris(&r) {
                let awal: String = r.chars().take(1500).collect();
                if let Ok((t, _, _)) = crate::terjemah::terjemahkan(http, &awal, Some("id")).await {
                    return cut(&format!("🌐 {t}

— teks asli —
{r}"), 3000);
                }
            }
            r
        }
        "pelajari" => {
            let p = arg.to_string();
            tokio::task::spawn_blocking(move || crate::teknik::pelajari(&p)).await.unwrap_or_else(|_| "gagal".into())
        }
        "ps" => sh(if cfg!(windows) { "tasklist" } else { "ps aux | head -50" }).await,
        "ls" => match std::fs::read_dir(if arg.is_empty() { "." } else { arg }) {
            Ok(rd) => rd
                .flatten()
                .take(200)
                .map(|e| {
                    let d = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    format!("{}{}", e.file_name().to_string_lossy(), if d { "/" } else { "" })
                })
                .collect::<Vec<_>>()
                .join("\n"),
            Err(e) => format!("gagal: {e}"),
        },
        "read" => match std::fs::read(arg) {
            Ok(b) => String::from_utf8_lossy(&b[..b.len().min(20_000)]).to_string(),
            Err(e) => format!("gagal: {e}"),
        },
        "fetch" => match http.get(arg).send().await {
            Ok(r) => match r.text().await {
                Ok(t) => strip_tags(&t),
                Err(e) => format!("gagal: {e}"),
            },
            Err(e) => format!("gagal: {e}"),
        },
        "shell" => sh(arg).await,
        "write" => {
            let (path, body) = c.arg.split_once('\n').unwrap_or((arg, ""));
            if body.len() > 100_000 {
                "gagal: isi terlalu besar (>100KB)".into()
            } else {
                crate::jurnal::before_write(path.trim());
                match std::fs::write(path.trim(), body) {
                    Ok(_) => format!("ditulis {} byte ke {}", body.len(), path.trim()),
                    Err(e) => format!("gagal: {e}"),
                }
            }
        }
        "open" if arg.contains("wikipedia.org/w/index.php?search=") => {
            // Diminta mencari di Wikipedia: artikelnya juga dimasukkan ke otak, lalu halamannya dibuka.
            let q = urlencoding::decode(arg.split("search=").nth(1).unwrap_or("")).map(|c| c.into_owned()).unwrap_or_default();
            let r = crate::wikipedia::cari_simpan(http, &q).await;
            crate::musik::buka_url(arg);
            format!("{r} (halaman dibuka di browser)")
        }
        "open" if arg.starts_with("https://www.youtube.com/results?search_query=") => {
            let q = urlencoding::decode(arg.trim_start_matches("https://www.youtube.com/results?search_query=")).map(|c| c.into_owned()).unwrap_or_default();
            crate::musik::catat(&q);
            // Pencarian musik: langsung putar video teratas (bukan berhenti di daftar hasil).
            let musik = q.to_lowercase().split_whitespace().any(|w| ["lagu", "musik", "music", "lofi", "playlist"].contains(&w));
            if musik {
                let q2 = q.clone();
                let _ = tokio::task::spawn_blocking(move || crate::musik::putar(&q2)).await;
                return format!("memutar di YouTube: {q}");
            }
            crate::musik::buka_url(arg);
            format!("dibuka: {arg}")
        }
        "open" if arg.starts_with("https://") || arg.starts_with("http://") => {
            // URL dibuka lewat penangan protokol Windows tanpa shell (aman dari injeksi perintah)
            let a = arg.to_string();
            let mut c = std::process::Command::new("rundll32");
            c.args(["url.dll,FileProtocolHandler", &a]);
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                c.creation_flags(0x0800_0000);
            }
            let _ = c.spawn();
            format!("dibuka: {arg}")
        }
        "open" if !arg.contains(['/', '\\', ':', '.']) && !arg.is_empty() => {
            // Nama aplikasi (bukan URL/path): cari di indeks menu Start, lalu PATH; laporkan jujur bila tidak ada.
            let q = arg.to_string();
            tokio::task::spawn_blocking(move || {
                if let Some(a) = crate::aplikasi::cari(&q) {
                    return if crate::aplikasi::buka(&a) { format!("dibuka: {}", a.nama) } else { format!("gagal membuka {}", a.nama) };
                }
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    let ada = std::process::Command::new("where").arg(&q).creation_flags(0x0800_0000).output().map(|o| o.status.success()).unwrap_or(false);
                    if ada {
                        let mut c = std::process::Command::new("cmd");
                        c.raw_arg(format!("/C start \"\" \"{q}\"")).creation_flags(0x0800_0000);
                        let _ = c.spawn();
                        return format!("dibuka: {q}");
                    }
                }
                format!("Tidak menemukan aplikasi '{q}' di laptop ini. Ketik /aplikasi {q} untuk melihat yang terpasang.")
            })
            .await
            .unwrap_or_else(|_| "gagal membuka aplikasi".into())
        }
        "open" => {
            #[cfg(windows)]
            {
                // raw_arg: cmd.exe butuh kutip apa adanya (args() akan meng-escape jadi \"\" dan memunculkan galat '\\').
                use std::os::windows::process::CommandExt;
                let a = arg.to_string();
                let _ = tokio::task::spawn_blocking(move || {
                    let mut c = std::process::Command::new("cmd");
                    c.raw_arg(format!("/C start \"\" \"{a}\"")).creation_flags(0x0800_0000);
                    c.spawn()
                })
                .await;
                return cut(&format!("dibuka: {arg}"), 3000);
            }
            #[allow(unreachable_code)]
            let cmd = if cfg!(windows) {
                format!("start \"\" \"{arg}\"")
            } else if cfg!(target_os = "macos") {
                format!("open '{arg}'")
            } else {
                format!("xdg-open '{arg}'")
            };
            sh(&cmd).await;
            format!("dibuka: {arg}")
        }
        t => format!("alat tidak dikenal: {t}"),
    };
    cut(&out, 3000)
}

/// Ambil objek JSON pertama dari balasan model (model kecil sering menambah teks di sekitarnya).
pub fn parse_json(t: &str) -> Option<serde_json::Value> {
    let (a, b) = (t.find('{')?, t.rfind('}')?);
    serde_json::from_str(t.get(a..=b)?).ok()
}

/// Teks dari PDF (atau file teks biasa), maks 300.000 karakter (aman untuk RAM kecil).
pub(crate) fn read_doc(path: &str) -> Result<String, String> {
    read_doc_max(path, 300_000)
}

/// PDF per halaman lewat lopdf: halaman yang gagal dilewati (PDF tebal/aneh sering membuat pdf-extract panik di satu halaman).
fn pdf_per_halaman(path: &str, max: usize) -> Result<String, String> {
    // Pengurai toleran sendiri dulu (peta ToUnicode rusak tidak menggagalkan halaman); lopdf bawaan sebagai cadangan.
    if let Ok(t) = crate::pdfteks::dokumen(path, max) {
        if t.trim().len() > 200 {
            return Ok(t);
        }
    }
    let doc = lopdf::Document::load(path).map_err(|e| e.to_string())?;
    let mut out = String::new();
    for n in doc.get_pages().keys().copied().collect::<Vec<u32>>() {
        if out.chars().count() >= max {
            break;
        }
        if let Ok(Ok(t)) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| doc.extract_text(&[n]))) {
            out.push_str(t.trim());
            out.push_str("\n\n");
        }
    }
    Ok(out)
}

/// Seperti `read_doc` tetapi dengan batas karakter sendiri (buku tebal perlu jauh lebih dari 300.000).
pub(crate) fn read_doc_max(path: &str, max: usize) -> Result<String, String> {
    let path = path.trim();
    let lower = path.to_ascii_lowercase();
    let text = if lower.ends_with(".pdf") {
        let p = path.to_string();
        let utama = std::panic::catch_unwind(move || pdf_extract::extract_text(&p)).ok().and_then(|r| r.ok()).filter(|t| t.trim().len() > 200);
        match utama {
            Some(t) => t,
            None => pdf_per_halaman(path, max)?,
        }
    } else {
        String::from_utf8_lossy(&std::fs::read(path).map_err(|e| e.to_string())?).into_owned()
    };
    if text.trim().is_empty() {
        return Err("tidak ada teks (PDF hasil scan/gambar butuh OCR)".into());
    }
    Ok(text.chars().take(max).collect())
}

const STOP: &[&str] = &[
    "yang", "dan", "di", "ke", "dari", "ini", "itu", "untuk", "pada", "dengan", "adalah", "dalam", "atau", "juga", "akan", "oleh", "sebagai",
    "the", "of", "and", "to", "in", "a", "is", "for", "that", "on", "with", "as", "by", "are", "be", "this", "an", "it", "or", "at", "from",
];

/// Ringkasan ekstraktif: pilih kalimat berskor frekuensi kata tertinggi, urut sesuai dokumen. Tanpa model.
pub fn summarize(text: &str, n: usize) -> String {
    use std::collections::HashMap;
    let sents: Vec<String> = text
        .split(|c| matches!(c, '.' | '!' | '?' | '\n'))
        .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|s| s.chars().count() > 30)
        .collect();
    let words = |s: &str| -> Vec<String> {
        s.split(|c: char| !c.is_alphanumeric()).map(|w| w.to_lowercase()).filter(|w| w.len() > 2 && !STOP.contains(&w.as_str())).collect()
    };
    let mut freq: HashMap<String, f32> = HashMap::new();
    for s in &sents {
        for w in words(s) {
            *freq.entry(w).or_default() += 1.0;
        }
    }
    let mut scored: Vec<(usize, f32)> = sents
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let w = words(s);
            let sc = w.iter().map(|x| freq[x]).sum::<f32>() / (w.len().max(1) as f32).sqrt();
            (i, sc)
        })
        .collect();
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut pick: Vec<usize> = scored.into_iter().take(n).map(|x| x.0).collect();
    pick.sort();
    let body = pick.iter().map(|&i| format!("- {}", sents[i].chars().take(300).collect::<String>())).collect::<Vec<_>>().join("\n");
    format!("({} kata, {} kalimat; ringkasan ekstraktif)\n{}", text.split_whitespace().count(), sents.len(), body)
}

/// Rencana tanpa model: kata kunci -> alat. None jika ada bagian yang tidak dipahami.
pub fn plan_offline(goal: &str) -> Option<Vec<Call>> {
    FROM_ML.store(false, Ordering::Relaxed);
    let mut segs = vec![goal.to_string()];
    for sep in [" lalu ", " kemudian ", " terus ", " lantas ", " setelah itu ", " habis itu ", " abis itu ", " selanjutnya ", ";", " then "] {
        segs = segs.iter().flat_map(|s| s.split(sep).map(String::from).collect::<Vec<_>>()).collect();
    }
    segs.retain(|s| !s.trim().is_empty());
    let one = segs.len() == 1;
    let mut calls = vec![];
    for s in &segs {
        let s = s.trim();
        match crate::aturan::rule(s) {
            Some(c) => {
                // Belajar online: kalimat ini sekarang menjadi contoh untuk niat tersebut.
                if one {
                    if let Some(l) = label_of(&c.tool) {
                        crate::ml::learn(l, s);
                    }
                }
                calls.push(c);
            }
            // Cadangan ML: klasifikasi niat dari kalimat bebas.
            None if one => {
                if let Some((label, _)) = crate::ml::classify(s) {
                    FROM_ML.store(true, Ordering::Relaxed);
                    calls.push(call_for(&label, s)?);
                } else if weak_tanya(s) {
                    calls.push(Call { tool: "tanya".into(), arg: s.to_string() });
                } else {
                    return None;
                }
            }
            None => return None,
        }
    }
    (!calls.is_empty()).then_some(calls)
}

/// Kalimat bernada pertanyaan yang tidak dikenali aturan maupun ML -> tetap dicari di memori.
fn weak_tanya(s: &str) -> bool {
    let w = s.to_lowercase();
    let first = w.split(|c: char| !c.is_alphanumeric()).find(|x| !x.is_empty()).unwrap_or("");
    s.trim_end().ends_with('?') || ["apa", "bagaimana", "berapa", "kenapa", "mengapa", "jelaskan", "pilih", "resistor", "drop", "cara", "mengapa", "kapan", "siapa"].contains(&first)
}

pub fn label_of(tool: &str) -> Option<&'static str> {
    Some(match tool {
        "tanya" => "tanya",
        "health" => "kesehatan",
        "ps" => "proses",
        "baterai" => "baterai",
        "agenda" => "agenda",
        "habit" => "habit",
        "tidur" => "tidur",
        "layar" => "layar",
        "ngobrol" => "ngobrol",
        "berita" => "berita",
        "kurikulum" => "kurikulum",
        "bukahasil" => "cari",
        "tugas" => "tugas",
        "atur" => "atur",
        "winapp" => "buka",
        "waktu" => "waktu",
        "cli" => "cli",
        "bukafile" => "cari",
        "wifi" | "port" => "cek",
        "jadwal" => "tidur",
        "otomatis" => "atur",
        "bukapath" => "cari",
        "kode" => "ketik",
        "gambar" | "jurnal" | "wiki" | "berpikir" | "terjemah" | "halaman" | "peramban" | "csv" => "baca",
        "gitringkas" => "cek",
        "harga" | "pantau" | "cuacapantau" => "web",
        "hitung" | "konversi" | "ohm" | "fase3" | "hari" | "zona" | "password" => "cek",
        "catat" | "catatan" => "alarm",
        "cuaca" | "kurs" => "web",
        "bersih" => "sapu",
        "web" => "web",
        "musik" => "musik",
        "email" => "email",
        "apps" => "apps",
        "rapikan" => "rapikan",
        "cari" => "cari",
        "meme" => "meme",
        _ => return None,
    })
}

pub fn call_for(label: &str, text: &str) -> Option<Call> {
    let (tool, arg) = match label {
        "tanya" => ("tanya", text),
        "kesehatan" => ("health", ""),
        "proses" => ("ps", ""),
        "baterai" => ("baterai", ""),
        "agenda" => ("agenda", ""),
        "habit" => ("habit", ""),
        "tidur" => ("tidur", ""),
        "layar" => ("layar", ""),
        "ngobrol" => ("ngobrol", text),
        "berita" => ("berita", ""),
        "email" => ("email", ""),
        "tugas" => ("tugas", ""),
        "waktu" => ("waktu", ""),
        "musik" => ("musik", ""),
        "apps" => ("apps", ""),
        "rapikan" => ("rapikan", text),
        "cari" => ("cari", text),
        "meme" => ("meme", text),
        "kurikulum" => ("kurikulum", ""),
        "wifi" => ("wifi", ""),
        "port" => ("port", ""),
        "jadwal" => ("jadwal", ""),
        _ => return None,
    };
    Some(Call { tool: tool.into(), arg: arg.into() })
}

/// Obrolan ringan tanpa model.
fn ngobrol(t: &str) -> String {
    let l = t.to_lowercase();
    let h = chrono::Timelike::hour(&chrono::Local::now());
    let _ = h;
    let sapa = crate::sistem::salam();
    if l.contains("terima kasih") || l.contains("makasih") {
        "Sama-sama, Bos! Kabari kalau butuh yang lain.".into()
    } else if l.contains("siapa kamu") || l.contains("kamu siapa") {
        format!("Aku {}, asisten pribadi Bos. Di dalamku ada 4 persona: NOVA (koordinator), VOLT (teknik dan kode), SENTRY (kesehatan laptop dan keamanan), MEMO (ingatan dan jadwal).", crate::agents::NAME)
    } else if l.contains("bisa apa") || l.contains("bantu") {
        "Aku bisa: cek kesehatan/baterai laptop, pantau kebiasaan dan mengingatkan jadwal/minum/tidur, jawab pertanyaan teknik listrik dan pemrograman, hitung (drop tegangan, kabel/MCB, dll.), ringkas PDF, baca layar (OCR), membantu saat Bos menyalin error di VS Code, dan memberi tahu email/kalender Google. Ketik /bantuan untuk daftar perintah.".into()
    } else if l.contains("apa kabar") {
        format!("Baik! {}", crate::monitor::summary().split(" | ").take(3).collect::<Vec<_>>().join(" · "))
    } else if ["halo", "hai", "hi", "selamat", "pagi", "siang", "sore", "malam"].iter().any(|k| l.split_whitespace().any(|w| w == *k)) {
        format!("Selamat {sapa}, Bos! Ada yang bisa kubantu?")
    } else {
        "Siap, Bos. Ketik /bantuan untuk melihat yang bisa kulakukan.".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn c(t: &str, a: &str) -> Call {
        Call { tool: t.into(), arg: a.into() }
    }
    #[test]
    fn youtube_and_fillers() {
        let p = plan_offline("halo nova coba buka youtube dan carikan video anime terbaru").unwrap();
        assert_eq!(p[0].tool, "open");
        assert!(p[0].arg.starts_with("https://www.youtube.com/results?search_query=anime"));
        assert!(matches!(assess(&p[0]), Risk::Safe));
        assert!(matches!(assess(&c("open", "https://x.id/a&b=1")), Risk::Block(_)));
        assert_eq!(plan_offline("kalo aku tidur sekarang maka rencananya jadi seperti apa").unwrap()[0].arg, "sekarang");
        assert_eq!(plan_offline("coba carikan gambar relay di internet").unwrap()[0].arg, "relay");
    }

    /// Penjaga urutan aturan: tiap kalimat harus menghasilkan alat yang diharapkan (aturan baru tak boleh menimpa yang lama).
    #[test]
    fn ordered_steps() {
        let p = plan_offline("buka youtube cari lagu santai setelah itu cek baterai lalu agenda").unwrap();
        let tools: Vec<&str> = p.iter().map(|c| c.tool.as_str()).collect();
        assert_eq!(tools, ["open", "baterai", "agenda"]);
    }
    #[test]
    fn web_services() {
        let url = |t: &str| crate::aturan::rule(t).map(|c| (c.tool, c.arg));
        let (tool, arg) = url("carikan di video youtube anime fullmetal").unwrap();
        assert_eq!(tool, "open");
        assert!(arg.starts_with("https://www.youtube.com/results?search_query=anime%20fullmetal"));
        assert!(url("kamu ganti music ke lagu indonesia").unwrap().1.contains("search_query=lagu%20indonesia"));
        assert!(url("tolong putarkan lagu dangdut koplo").unwrap().1.contains("lagu%20dangdut%20koplo"));
        // data JSON: menambah layanan dan sinonim aksi tanpa mengubah kode
        assert!(crate::aturan::layanan_tambah("wikipedia wiki https://id.wikipedia.org/w/index.php?search={q}").contains("ditambahkan"));
        assert!(url("cari motor induksi di wiki").unwrap().1.contains("search=motor%20induksi"));
        assert!(!crate::kata::punya_aksi("dengerin lagu", "putar"));
        crate::kata::tambah_aksi("putar", &["dengerin"]);
        assert!(crate::kata::punya_aksi("dengerin lagu", "putar"));
        for t in ["buka music anime", "buka lagu anime", "putar lagu anime", "nyalakan musik anime"] {
            assert!(url(t).unwrap().1.contains("search_query=lagu%20anime"), "{t}");
        }
        assert!(!url("buka spotify music").is_some_and(|c| c.1.contains("youtube")));
        assert!(url("rute ke bandung").unwrap().1.contains("google.com/maps/search/bandung"));
        assert!(url("carikan repo rust pet di github").unwrap().1.contains("github.com/search"));
        assert!(url("buka video anime fullmetal").unwrap().1.contains("youtube.com/results?search_query=anime%20fullmetal"));
        assert_ne!(url("buka video.mp4").map(|c| c.1.contains("youtube")), Some(true));
        assert_ne!(url("carikan file video mp4 kemarin").map(|c| c.0), Some("open".into()));
    }
    #[test]
    fn rules_golden() {
        let cases = [
            ("hitung 2+3*4", "hitung"),
            ("cuaca di bandung", "cuaca"),
            ("harga bitcoin", "harga"),
            ("catat beli kabel besok", "catat"),
            ("jam 14:00 wib di london", "zona"),
            ("ohm V=12 R=4", "ohm"),
            ("berapa hari lagi ke 17 agustus", "hari"),
            ("pantau bbca di bawah 1000", "pantau"),
            ("ingatkan kalau hujan di bandung", "cuacapantau"),
            ("kalo aku tidur sekarang maka rencananya jadi seperti apa", "jadwal"),
            ("bisa tidur 8 jam malam ini", "jadwal"),
            ("coba carikan gambar relay di internet", "gambar"),
            ("carikan jurnal semantic 2021 sampai 2025", "jurnal"),
            ("buka vs code folder digital nameplate", "kode"),
            ("bersihkan cache aman", "bersih"),
            ("halo nova coba buka youtube dan carikan video anime terbaru", "open"),
            ("analisis csv C:/a.csv", "csv"),
            ("wikipedia transformator", "wiki"),
            ("konversi 5 km ke mil", "konversi"),
            ("cek port yang aktif", "port"),
            ("cek wifi disini apa aja", "wifi"),
            ("buka kalkulator", "winapp"),
        ];
        for (q, tool) in cases {
            let p = plan_offline(q).unwrap_or_default();
            assert_eq!(p.first().map(|c| c.tool.as_str()), Some(tool), "kalimat: {q}");
        }
    }
    #[test]
    fn risk_levels() {
        assert!(matches!(assess(&c("info", "")), Risk::Safe));
        assert!(matches!(assess(&c("shell", "dir")), Risk::Safe));
        assert!(matches!(assess(&c("shell", "mkdir x")), Risk::Confirm(_)));
        assert!(matches!(assess(&c("shell", "type C:/u/.ssh/id_rsa")), Risk::Confirm(_)));
        assert!(matches!(assess(&c("shell", "dir > a.txt")), Risk::Confirm(_)));
        assert!(matches!(assess(&c("shell", "rm -rf /")), Risk::Block(_)));
        assert!(matches!(assess(&c("read", "C:/u/.ssh/id_rsa")), Risk::Confirm(_)));
        assert!(matches!(assess(&c("open", "a&calc")), Risk::Block(_)));
        assert!(matches!(assess(&c("xx", "")), Risk::Block(_)));
    }
    #[test]
    fn offline_rules() {
        let p = plan_offline("isi folder C:/Users/me/Downloads lalu cek kesehatan laptop").unwrap();
        assert_eq!((p[0].tool.as_str(), p[0].arg.as_str()), ("ls", "C:/Users/me/Downloads"));
        assert_eq!(p[1].tool, "health");
        assert_eq!(plan_offline("buka https://rust-lang.org").unwrap()[0].tool, "open");
        assert_eq!(plan_offline("jalankan ipconfig").unwrap()[0].arg, "ipconfig");
        assert!(plan_offline("tolong buatkan puisi").is_none());
    }
    #[test]
    fn summary_picks_sentences() {
        let t = "Rust adalah bahasa pemrograman sistem yang cepat dan aman. Rust memakai ownership untuk keamanan memori. \
Cuaca hari ini cerah sekali di kota bandung dan sekitarnya. Ownership di Rust mencegah data race pada program konkuren.";
        let s = summarize(t, 2);
        assert!(s.contains("Rust"));
        assert_eq!(plan_offline("ringkas pdf C:/a/b.pdf").unwrap()[0].tool, "ringkas");
    }
    #[test]
    fn json_extraction() {
        assert!(parse_json("ok {\"a\":1} bye").is_some());
        assert!(parse_json("tanpa json").is_none());
    }
}
