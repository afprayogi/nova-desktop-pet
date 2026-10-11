<p align="center"><img src="docs/banner.svg" alt="NOVA" width="720"></p>

# NOVA

Asisten desktop berwujud peliharaan pixel-art untuk Windows, ditulis dalam Rust. NOVA berjalan di tray, hidup di atas desktop, dan
membantu pekerjaan harian: perintah komputer, jadwal, ingatan, analitik kebiasaan, dan asisten koding. Hampir semua fitur bekerja
**offline dan lokal**; model bahasa (Gemini, OmniRoute, atau Ollama) bersifat opsional.

> Status: pengembangan aktif, hanya Windows 10/11. Antarmuka dan sebagian besar teks berbahasa Indonesia.

## Fitur

| Area | Isi |
|------|-----|
| Peliharaan | Jendela transparan (egui), animasi, karakter bergaya Shimeji, sahabat ber-level (buddy), dok ketik, popup balasan |
| Perintah komputer | Cari/rapikan berkas (pratinjau + `urungkan`), proses, baterai, kecerahan/volume, buka aplikasi (daftar putih), baca layar (OCR), shell terbatas |
| Otak berlapis | Aturan lokal → router semantik (embedding) → Gemini (JSON) → OmniRoute → Llama lokal (Ollama), dengan koreksi-diri bila JSON rusak |
| Memori | SQLite lokal: fakta tentang pengguna, pencarian hibrida BM25 + vektor + graf, tindak-lanjut obrolan multi-giliran |
| Analitik | K-Means tipe hari, deteksi anomali, ramalan disk, prediksi kelelahan (MLP), prediksi baterai |
| Web | Cuaca, kurs, berita sesuai minat, jadwal sholat, harga (Yahoo), baca halaman lewat Chrome headless |
| Email | Baca (Gmail/Outlook OAuth, IMAP), kirim laporan ke diri sendiri (SMTP, selalu minta persetujuan) |
| Koding | `/proyek`, `/log`, `/makro`, `/scaffold`, `/build`, ringkasan git, ekstensi browser pemblokir iklan |
| Keamanan | Penilaian risiko tiap alat (aman/konfirmasi/blok), daftar jalur sensitif, kunci dienkripsi DPAPI, API lokal bertoken |

Daftar lengkap perintah: ketik `/bantuan` di NOVA.

## Mulai cepat

Prasyarat: Windows 10/11 dan [Rust](https://rustup.rs) (stable).

```powershell
git clone <url-repo> nova
cd nova
cargo run --release          # tray + peliharaan
```

Pasang permanen (tanpa admin, ke `%LOCALAPPDATA%\NOVA` + mulai otomatis): jalankan `pasang-asisten.bat`; lepas dengan `lepas-asisten.bat`.
Pintasan global **Ctrl+Alt+N** membuka obrolan cepat.

### Model bahasa (opsional)
- **Gemini**: `/gemini key <kunci>` (disimpan terenkripsi).
- **OmniRoute / Ollama**: atur lewat Pengaturan, atau `/ollama` dan `/lokal`. Model kecil `llama3.2:1b` dan `nomic-embed-text` cukup untuk RAM 8 GB.
- **Email**: `/google`, `/outlook`, `/email imap auto <alamat> <sandi-aplikasi>`, `/smtp auto <alamat> <sandi-aplikasi>`.

## Arsitektur

<p align="center"><img src="docs/arsitektur.svg" alt="Diagram arsitektur NOVA" width="800"></p>

```
src/main.rs        entri: tray, API, peliharaan
src/engine.rs      inti: antrean misi, agen, event
src/engine/        chat · slash (perintah /) · misi (perencana) · teks (bantuan)
src/aturan.rs      aturan bahasa alami → alat       src/alat.rs   alat + penilaian risiko
src/gemini.rs, omniroute.rs, lokal.rs               rantai penyedia model
src/niat.rs        klasifikasi niat terstruktur     src/rawat.rs  normalisasi + tindak-lanjut
src/pet.rs, lukis.rs                                 jendela & gambar peliharaan (egui)
src/api.rs, ui.html, dash.html                       API lokal 127.0.0.1 + UI web
```

API lokal hanya menerima `localhost` dengan token (`x-nova-token`) yang dibuat di folder data.

## Pengembangan

```powershell
cargo test                   # 120+ tes (data uji terisolasi di folder sementara)
cargo build --release
```

Matikan `nova.exe` yang sedang berjalan sebelum membangun ulang rilis (berkas terkunci).

## Privasi

Data hanya disimpan lokal di `%APPDATA%\nova` (hapus folder itu untuk menghapus semuanya): nama aplikasi aktif + jam, fakta yang Anda
simpan, riwayat obrolan, statistik. Tidak ada ketikan global, judul jendela, atau isi layar yang disimpan. OCR mati bawaan. Kunci, token,
dan sandi dienkripsi DPAPI (hanya akun Windows Anda). Ke penyedia model hanya dikirim teks perintah dan konteks ringkas, tidak pernah isi berkas, layar, atau email.

## Aset karakter

Repositori **tidak** menyertakan seni karakter. Paket karakter diletakkan di `%APPDATA%\nova\karakter\<nama>\`; buat sendiri dari gambar Anda
dengan `/karakter buat <nama> <gambar>`. Seni berhak cipta pihak lain hanya untuk pemakaian pribadi dan tidak boleh disebarkan.

## Lisensi & atribusi

[MIT](LICENSE). Bagian yang diadaptasi dicatat di [NOTICE.md](NOTICE.md).
