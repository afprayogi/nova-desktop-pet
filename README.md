<p align="center"><img src="docs/banner.svg" alt="NOVA" width="720"></p>

# NOVA

NOVA adalah peliharaan pixel-art yang tinggal di desktop Windows-ku dan bisa diajak ngobrol. Aku mulai membuatnya karena bosan
dengan asisten yang semuanya lewat cloud: aku ingin yang ingatannya ada di laptopku sendiri, bisa disuruh membereskan berkas dan
mengecek baterai, dan tetap berguna waktu internet mati. Ditulis dengan Rust, dan masih terus kutambal.

Hanya jalan di Windows 10/11. Teksnya berbahasa Indonesia.

## Yang bisa dilakukan

Kebanyakan hal berjalan tanpa model bahasa sama sekali. Kalimat yang kukenali lewat aturan dan pencocokan makna (embedding)
langsung dikerjakan di laptop. Model baru dipanggil kalau kalimatnya tidak dikenali: Gemini dulu, lalu OmniRoute, lalu Llama
lokal lewat Ollama. Kalau Llama membalas JSON yang rusak, galatnya dikembalikan ke modelnya supaya diperbaiki.

Beberapa hal yang sehari-hari kupakai:

- Mencari dan merapikan berkas. Perubahan selalu dipratinjau dulu dan bisa dibatalkan dengan `urungkan`.
- Mengecek proses, baterai, CPU, RAM, disk; mengatur kecerahan dan volume; membuka aplikasi dari daftar yang diizinkan.
- Mengingat fakta tentangku, jadwal, dan kebiasaan. Semuanya di SQLite lokal. Obrolan lanjutan seperti "yang rilis tahun ini aja"
  nyambung ke pertanyaan sebelumnya.
- Cuaca, kurs, berita sesuai minat, jadwal sholat, harga saham, dan membaca halaman web lewat Chrome tanpa jendela.
- Membaca email (Gmail, Outlook, IMAP) dan mengirim ringkasan ke alamatku sendiri, selalu setelah kutekan setuju.
- Bantuan koding: `/proyek`, `/log`, `/makro`, `/build`, ringkasan git, dan pemblokir iklan lewat DNS lokal plus ekstensi browser.
- Statistik pemakaian: tipe hari, anomali, ramalan disk, dan perkiraan kapan aku mulai kelelahan.

Daftar lengkapnya muncul kalau mengetik `/bantuan` di NOVA.

## Mencoba

Butuh Windows dan [Rust](https://rustup.rs).

```powershell
git clone https://github.com/afprayogi/nova-desktop-pet.git
cd nova-desktop-pet
cargo run --release
```

Supaya terpasang tetap dan mulai sendiri saat login (tanpa hak admin), jalankan `pasang-asisten.bat`. Untuk melepasnya,
`lepas-asisten.bat`. Ctrl+Alt+N membuka obrolan cepat.

Model bahasa bersifat opsional. Kunci Gemini dimasukkan dengan `/gemini key <kunci>` dan disimpan terenkripsi. Untuk model lokal,
pasang [Ollama](https://ollama.com) lalu pakai `/ollama` dan `/lokal`; `llama3.2:1b` sudah cukup di laptop 8 GB. Email diatur lewat
`/google`, `/outlook`, `/email imap` dan `/smtp`.

## Cara kerjanya

<p align="center"><img src="docs/arsitektur.svg" alt="Diagram arsitektur NOVA" width="800"></p>

Peliharaan dan antarmuka web hanyalah klien dari API lokal di `127.0.0.1` yang dijaga token. Semua permintaan masuk ke
`src/engine.rs`, yang memutuskan lewat rantai di atas. Sebelum alat apa pun dijalankan, `alat::assess` menilainya: aman,
perlu konfirmasi, atau ditolak. Model hanya memilih nama alat; yang menjalankannya kode Rust dengan daftar putih.

Peta singkat isi `src/`: `engine/` (obrolan, perintah `/`, perencana misi), `aturan.rs` dan `alat.rs` (aturan bahasa dan alat),
`gemini.rs`, `omniroute.rs`, `lokal.rs` (penyedia model), `niat.rs` dan `rawat.rs` (klasifikasi niat dan normalisasi kalimat),
`pet.rs` dan `lukis.rs` (jendela dan gambar peliharaan).

## Mengembangkan

```powershell
cargo test
cargo build --release
```

Tutup `nova.exe` yang sedang jalan sebelum membangun rilis, kalau tidak berkasnya terkunci.

## Privasi

Data tinggal di `%APPDATA%\nova`; hapus folder itu dan semuanya hilang. Yang dicatat: nama aplikasi yang sedang aktif beserta jam,
fakta yang kusimpan, riwayat obrolan, dan statistik. Tidak ada ketikan global, judul jendela, atau isi layar yang disimpan, dan OCR
mati secara bawaan. Kunci, token, dan sandi dienkripsi dengan DPAPI Windows. Ke penyedia model hanya dikirim teks perintah dan
konteks singkat, tidak pernah isi berkas, layar, atau email.

## Karakter

Repositori ini tidak menyertakan gambar karakter apa pun. Paket karakter bergaya Shimeji ditaruh di
`%APPDATA%\nova\karakter\<nama>\`, atau dibuat dari gambarmu sendiri dengan `/karakter buat <nama> <gambar>`. Seni milik orang lain
hanya untuk dipakai sendiri dan jangan disebarkan.

## Lisensi

[MIT](LICENSE). Bagian yang kuadaptasi dari proyek lain dicatat di [NOTICE.md](NOTICE.md).
