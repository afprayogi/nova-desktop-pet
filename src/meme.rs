//! Meme & emotikon offline: bank meme teks (programmer, listrik, kantor) + kaomoji ASCII + emoji aman untuk font bawaan.
//! Dipakai pada balasan humor, obrolan antar agen, dan perintah /meme. Murni daftar lokal, tanpa internet.

const BANK: &[(&str, &[&str])] = &[
    ("error", &[
        "Aku: *menulis kode 5 menit*\nCompiler: 47 error 🙃",
        "Kode jalan di laptopku 😎\nKode di laptop lain: 💥 (itu bukan fix, itu env)",
        "Bug: ada.\nLog: kosong.\nAku: (>_<) *menatap layar 3 jam*",
    ]),
    ("deadline", &[
        "Deadline: besok.\nAku: baru buka file-nya 😎🔥",
        "Estimasi: 2 jam.\nRealita: 2 hari + 1 kopi ☕ + 1 krisis eksistensial",
        "Tugas: 99% selesai.\nSisa 1%: 99% waktunya (T_T)",
    ]),
    ("sukses", &[
        "Build hijau! ✅🎉\nAku: \\(^o^)/ *joget pixel*",
        "Sekali jalan langsung benar?\nAku: curiga... ini pasti jebakan 😳",
        "Tes lulus semua ✅\nProduction: 'halo, aku punya data sendiri' 😈",
    ]),
    ("lelah", &[
        "Kopi: ☕ loading...\nSemangat: 404 not found 😴",
        "Pukul 2 pagi:\n'Satu bug lagi lalu tidur'\n(Bug ke-7, matahari terbit) 🌙",
        "Baterai 1% 🔋 vs Aku 1% 🥱\nSama-sama butuh charge",
    ]),
    ("git", &[
        "git commit -m 'fix'\ngit commit -m 'fix lagi'\ngit commit -m 'beneran fix' 😅",
        "git push --force 😈\nTim: ...\nAku: (-_-;) bercanda",
        "Branch: main\nAku: ngoding langsung di sini\nSENTRY: ✚ *memakai helm* ⚠",
    ]),
    ("listrik", &[
        "MCB trip lagi? Itu bukan bug, itu fitur proteksi 😌⚡",
        "Hukum Ohm: V = I x R\nHukum Murphy: kalau bisa salah, pasti salah di jam 2 pagi 🌙",
        "Fasa kena netral?\nAku: ⚡ *langsung teleport ke grounding*",
        "Kabel 1,5 mm² untuk motor 5 kW?\nKabel: 🔥🔥🔥 'tolong'",
    ]),
    ("plc", &[
        "Ladder logic: Rung 1 aktif, Rung 2 aktif...\nAku: kenapa output-nya diam? (o_O)",
        "PLC: 'RUN'\nOperator: 'kok tidak jalan?'\nE-stop: 🍄 *masih tertekan* 😅",
    ]),
    ("tidur", &[
        "Tidur 8 jam: 🛌\nAlasan 'tambah 1 commit lagi': 💀",
        "Aku: 'tidur jam 11'\nJam 11:01: 'sebentar, explore 1 video lagi' 😴",
    ]),
    ("umum", &[
        "Aku: 'ini cuma 5 menit'\nWaktu: *berlalu 3 jam* ⏳",
        "Programmer: 'tidak ada yang berubah'\nProgram: 💥",
        "Ctrl+C, Ctrl+V — keahlian utama sejak 1983 (^_^)",
        "Wifi: 'terhubung, tanpa internet' 😶\nAku: ya, sama kayak hidupku",
    ]),
];

/// Pilih meme menurut kata kunci topik; `seed` memilih variannya.
pub fn meme(topic: &str, seed: usize) -> String {
    let l = topic.to_lowercase();
    let key = if ["error", "bug", "gagal", "crash", "salah", "stres", "pusing"].iter().any(|k| l.contains(k)) {
        "error"
    } else if ["deadline", "tugas", "telat", "numpuk"].iter().any(|k| l.contains(k)) {
        "deadline"
    } else if ["berhasil", "sukses", "lulus", "mantap", "selesai", "hore"].iter().any(|k| l.contains(k)) {
        "sukses"
    } else if ["capek", "lelah", "ngantuk", "begadang", "penat"].iter().any(|k| l.contains(k)) {
        "lelah"
    } else if ["git", "commit", "branch", "push", "merge"].iter().any(|k| l.contains(k)) {
        "git"
    } else if ["listrik", "mcb", "kabel", "motor", "fasa", "ohm", "trafo", "tegangan"].iter().any(|k| l.contains(k)) {
        "listrik"
    } else if ["plc", "ladder", "scada", "hmi"].iter().any(|k| l.contains(k)) {
        "plc"
    } else if ["tidur", "malam", "begadang"].iter().any(|k| l.contains(k)) {
        "tidur"
    } else {
        "umum"
    };
    let set = BANK.iter().find(|(k, _)| *k == key).map(|x| x.1).unwrap_or(BANK[BANK.len() - 1].1);
    set[seed % set.len()].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn picks_by_topic() {
        assert!(meme("aku stres ada error", 0).contains("Compiler") || meme("error", 1).contains("laptop"));
        assert!(meme("mcb sering trip", 0).contains("MCB"));
        assert!(!meme("apa saja", 3).is_empty());
    }
}
