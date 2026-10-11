//! Basis solusi lokal berbasis regex (deterministik, tanpa model): mencocokkan pesan error dengan penyebab/solusi.
//! Dapat diperluas Bos lewat `solusi.json` di folder data: [{"pola":"regex","solusi":"teks, boleh pakai $1"}].
use regex::Regex;
use serde::Deserialize;

const BAWAAN: &[(&str, &str)] = &[
    (r"(?i)(?:address already in use|EADDRINUSE|port\s*(\d{2,5})\s*(?:is\s*)?(?:already\s*)?in use).*?(\d{2,5})?", "Port sedang dipakai proses lain (sering sisa proses lama). Ketik /port untuk melihat pemiliknya, lalu /killport <nomor> untuk mematikannya."),
    (r"E0382", "E0382 (Rust): nilai sudah dipindah (move). Pinjam dengan & atau gunakan .clone() sebelum dipakai lagi."),
    (r"E0499", "E0499 (Rust): dua &mut aktif bersamaan. Persempit scope peminjaman atau pecah struct."),
    (r"E0502", "E0502 (Rust): meminjam mutable saat masih ada pinjaman immutable. Selesaikan pemakaian & dulu, baru ubah."),
    (r"E0308", "E0308 (Rust): tipe tidak cocok. Periksa tipe kembalian/argumen; gunakan .into(), as, atau ubah anotasi tipe."),
    (r"E0425", "E0425 (Rust): nama tidak ditemukan. Cek ejaan, use/import, atau apakah variabel berada di scope."),
    (r"E0599", "E0599 (Rust): method tidak ada pada tipe itu. Periksa trait yang perlu di-use atau tipe sebenarnya."),
    (r"(?i)linker .* not found|LNK1181|cannot open input file", "Masalah linker: pasang build tools (Visual Studio Build Tools / gcc) atau pastikan pustaka yang ditautkan ada."),
    (r"(?i)ModuleNotFoundError: No module named '([\w\.]+)'", "Modul Python '$1' belum terpasang. Jalankan: pip install $1 (pastikan virtualenv yang benar aktif)."),
    (r"(?i)Cannot find module '([^']+)'", "Modul Node '$1' tidak ada. Jalankan: npm install $1 (atau npm install bila itu paket proyek)."),
    (r"(?i)IndentationError|TabError", "Indentasi Python salah. Gunakan 4 spasi konsisten, jangan campur tab dan spasi."),
    (r"(?i)SyntaxError: invalid syntax", "Sintaks tidak valid: cek tanda kurung/titik dua yang hilang di baris tersebut atau baris sebelumnya."),
    (r"(?i)KeyError: ?'?([\w\-]+)", "KeyError '$1': kunci tidak ada di dict. Gunakan dict.get('$1') atau periksa kunci dengan 'in'."),
    (r"(?i)list index out of range|IndexError", "Indeks di luar rentang list. Periksa len() sebelum mengakses, atau gunakan loop langsung."),
    (r"(?i)TypeError: .*(undefined|null)|Cannot read propert(?:y|ies) of (undefined|null)", "Objek undefined/null diakses. Periksa data sebelum dipakai, gunakan optional chaining (a?.b) dan nilai default."),
    (r"(?i)NullPointerException", "NullPointerException: referensi null diakses. Periksa null/Optional sebelum memanggil method."),
    (r"(?i)segmentation fault|SIGSEGV", "Segmentation fault: akses memori tidak valid (pointer NULL, buffer overflow, use-after-free). Jalankan dengan debugger/valgrind/AddressSanitizer."),
    (r"(?i)undefined reference to `?([\w:]+)", "Linker: '$1' dideklarasikan tapi tidak ditautkan. Tambahkan file sumber/pustaka (-l...) yang mendefinisikannya."),
    (r"(?i)permission denied|EACCES", "Izin ditolak: jalankan terminal sebagai admin bila perlu, atau periksa izin file/folder dan proses lain yang mengunci file."),
    (r"(?i)ECONNREFUSED", "Koneksi ditolak: server tujuan belum berjalan atau port salah. Nyalakan servernya dan cek host:port."),
    (r"(?i)CORS|Access-Control-Allow-Origin", "Error CORS: server harus mengirim header Access-Control-Allow-Origin untuk origin Anda (atau gunakan proxy dev)."),
    (r"(?i)fatal: not a git repository", "Folder ini bukan repositori git. Jalankan git init atau pindah ke folder proyek."),
    (r"(?i)non-fast-forward|Updates were rejected", "Push ditolak karena remote lebih baru. Jalankan git pull --rebase lalu push lagi (jangan force-push ke branch bersama)."),
    (r"(?i)Please commit your changes or stash them|would be overwritten by (?:merge|checkout)", "Ada perubahan lokal yang bentrok. Commit dulu atau simpan sementara dengan git stash."),
    (r"(?i)CONFLICT \(content\)|Merge conflict in", "Konflik merge: edit penanda <<<<<<< ======= >>>>>>> di file terkait, lalu git add dan git commit."),
    (r"(?i)running scripts is disabled|execution of scripts is disabled", "PowerShell memblokir skrip. Jalankan: Set-ExecutionPolicy -Scope CurrentUser RemoteSigned."),
    (r"(?i)'(\w+)' is not recognized as an internal or external command|command not found: (\w+)", "Perintah '$1' tidak dikenal: belum terpasang atau belum ada di PATH. Pasang alatnya lalu buka terminal baru."),
    (r"(?i)ENOENT|No such file or directory|FileNotFoundError", "File/folder tidak ditemukan. Periksa path (relatif terhadap folder kerja) dan ejaan nama file."),
    (r"(?i)out of memory|OutOfMemory|heap out of memory", "Kehabisan memori. Kurangi data yang dimuat sekaligus (stream/batch) atau naikkan batas memori runtime."),
    (r"(?i)stack overflow|maximum recursion depth", "Rekursi terlalu dalam. Pastikan ada kasus dasar yang tercapai, atau ubah menjadi iterasi."),
];

#[derive(Deserialize)]
struct Custom {
    pola: String,
    solusi: String,
}

fn custom() -> Vec<(Regex, String)> {
    std::fs::read_to_string(crate::config::data_dir().join("solusi.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Vec<Custom>>(&t).ok())
        .map(|v| v.into_iter().filter_map(|c| Regex::new(&c.pola).ok().map(|r| (r, c.solusi))).collect())
        .unwrap_or_default()
}

/// Cocokkan teks error dengan solusi lokal (milik Bos lebih dulu, lalu bawaan).
pub fn cari(text: &str) -> Option<String> {
    let t: String = text.chars().take(4000).collect();
    for (re, sol) in custom() {
        if let Some(c) = re.captures(&t) {
            let mut out = String::new();
            c.expand(&sol, &mut out);
            return Some(out);
        }
    }
    for (p, sol) in BAWAAN {
        if let Ok(re) = Regex::new(p) {
            if let Some(c) = re.captures(&t) {
                let mut out = String::new();
                c.expand(sol, &mut out);
                // Port yang tertangkap -> petunjuk spesifik.
                if sol.contains("/killport") {
                    if let Some(port) = Regex::new(r"(?:port|:)\s*(\d{4,5})").ok().and_then(|r| r.captures(&t)).and_then(|c| c.get(1)) {
                        out = format!("Port {} sedang dipakai proses lain. Ketik /port untuk melihat pemiliknya, lalu /killport {} untuk mematikannya.", port.as_str(), port.as_str());
                    }
                }
                return Some(out);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn regex_solutions() {
        assert!(cari("ModuleNotFoundError: No module named 'requests'").unwrap().contains("pip install requests"));
        assert!(cari("Error: listen EADDRINUSE: address already in use :::3000").unwrap().contains("/killport 3000"));
        assert!(cari("error[E0382]: borrow of moved value").unwrap().contains("move"));
        assert!(cari("Cannot find module 'express'").unwrap().contains("npm install express"));
        assert!(cari("semuanya baik baik saja").is_none());
    }
}
