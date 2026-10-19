# Merilis dan memperbarui NOVA (Windows)

NOVA dikirim sebagai **pemasang `NOVA-Setup-<versi>.exe`** (Inno Setup) dan **memperbarui dirinya sendiri** dari GitHub Releases.
Pembaruan tidak pernah dipasang tanpa persetujuan pengguna, dan hanya yang **bertanda tangan digital Anda** yang diterima.

## Menjalankan NOVA

| Tujuan | Cara |
|---|---|
| Mencoba dari kode | `cargo run --release` (jendela peliharaan + tray) |
| Dipasang seperti aplikasi biasa | `powershell -ExecutionPolicy Bypass -File installer\bangun.ps1`, lalu jalankan `dist\NOVA-Setup-<versi>.exe` |
| Setelah terpasang | Start Menu → **NOVA**; ikon di tray; pintasan **Ctrl+Alt+N** membuka obrolan |
| Tanpa memasang | `nova-portabel.exe` dari halaman Releases |

## Berjalan di latar: Windows Task Scheduler (bukan Windows Service)

NOVA memerlukan desktop Anda (tray, jendela peliharaan, pop-up, aplikasi aktif, clipboard), sedangkan *Windows Service* berjalan di
sesi terpisah tanpa desktop dan biasanya dengan hak SYSTEM. Karena itu NOVA berjalan **sebagai akun Anda** lewat Task Scheduler:

- Tugas `NOVA\Mulai`: pemicu *saat login*, hak terendah (tanpa admin), dan **dihidupkan lagi otomatis** (5 kali, jeda 1 menit) bila berhenti karena galat.
  Dinyalakan oleh pemasang (opsi "Jalankan NOVA otomatis"), menu tray, atau perintah `nyalakan nova otomatis`. Mematikan: `matikan nova otomatis`.
- Tugas buatan Anda: `NOVA\Tugas-<id>`, dijalankan sebagai `nova --tugas <id>` (tanpa jendela, lalu keluar).

### Meminta NOVA membuat jadwal

Ucapkan misalnya: `jadwalkan setiap hari jam 08:30 tampilkan cuaca Jakarta lalu agenda hari ini`
atau `setiap Jumat jam 16:00 ingatkan saya rapat mingguan`.
Model (Gemini, atau Llama lokal) merancang jadwal dan langkah dari **alat NOVA yang sudah ada**. NOVA memeriksa keamanannya, menampilkan
rancangan, dan baru membuat tugas Windows setelah Anda menekan **buat jadwal ini**.

Aturan keamanan tugas terjadwal:
- Hanya alat terdaftar; `shell`, `kirim`, `bersih`, `urungkan`, `pembaruan`, serta pembuat jadwal/keahlian dilarang.
- Langkah yang butuh izin **tidak dijalankan otomatis** (hanya diberitahukan lewat pop-up), jadi tidak ada tindakan berisiko diam-diam.
- Jenis jadwal: harian, mingguan (pilih hari), sekali, tiap N menit (15-1440), dan saat login. Maksimal 20 tugas.
- Hasil tampil sebagai pop-up dan masuk ke notifikasi NOVA; jejak diagnosis ada di `%TEMP%\nova-tugas.log`.

Perintah: `daftar tugas terjadwal`, `jalankan tugas terjadwal 1` (uji sekarang), `hapus tugas terjadwal 1`.

## Gambaran alur

```
git tag v0.3.1 ──push──▶ GitHub Actions (release.yml)
                          ├─ cargo test + cargo build --release
                          ├─ Inno Setup  ──▶ NOVA-Setup-0.3.1.exe
                          ├─ nova --rilis ──▶ latest.json  (versi, url, sha256, tanda tangan Ed25519)
                          └─ Releases: pemasang + latest.json + nova-portabel.exe

NOVA di laptop pengguna
  tiap 12 jam: GET …/releases/latest/download/latest.json
   ├─ tanda tangan cocok dengan pembaruan.pub (tertanam)?  tidak → TOLAK
   ├─ versi lebih tinggi?                                  tidak → abaikan (tak bisa downgrade)
   └─ ya → tanya pengguna: "Perbarui sekarang?"
         ├─ unduh (bar persen) → cocokkan SHA-256 → jalankan pemasang senyap
         └─ NOVA menutup diri → pemasang mengganti → NOVA menyala lagi
```

## Sekali saja (persiapan)

1. **Kunci penandatangan** sudah dibuat dengan `nova --buat-kunci`:
   - kunci publik: `pembaruan.pub` (di repo, tertanam ke aplikasi),
   - kunci privat: `%USERPROFILE%\.nova-rilis\kunci-privat.b64` (**jangan di-commit**).
2. Tambahkan secret di GitHub: *Settings → Secrets and variables → Actions → New repository secret*
   - Name: `NOVA_SIGNING_KEY`
   - Value: isi file `kunci-privat.b64`
3. **Cadangkan** `kunci-privat.b64` (pengelola kata sandi / flashdisk). Bila hilang, pengguna lama tidak bisa menerima pembaruan otomatis
   (mereka harus memasang manual versi yang memuat kunci publik baru). Bila bocor: buat kunci baru (`nova --buat-kunci`), rilis manual, dan anggap kunci lama tidak tepercaya.
4. Commit `pembaruan.pub`, `installer/`, dan `.github/workflows/release.yml`.

## Merilis versi baru (rutin)

1. Ubah `version` di `Cargo.toml` (mis. `0.3.1`) dan tambahkan catatan di `CHANGELOG.md`.
2. Commit, lalu:
   ```
   git tag v0.3.1
   git push origin main --tags
   ```
3. Tunggu workflow **release** selesai (±10–15 menit). Halaman *Releases* akan memuat pemasang dan `latest.json`.
4. Pengguna menerima notifikasi "NOVA versi 0.3.1 tersedia" dalam ≤12 jam (atau ketik **cek pembaruan**).

Workflow menolak bila tag tidak sama dengan versi di `Cargo.toml`, sehingga tidak ada rilis salah nomor.

## Mencoba di laptop sendiri

```
powershell -ExecutionPolicy Bypass -File installer\bangun.ps1
```
Hasil: `dist\NOVA-Setup-<versi>.exe` (dan `dist\latest.json` bila kunci privat ada di `%USERPROFILE%\.nova-rilis`).
Butuh Inno Setup 6 (`winget install JRSoftware.InnoSetup`).

Perintah di dalam NOVA: `versi nova`, `cek pembaruan`, `perbarui sekarang`.
Alat pengembang: `nova --versi`, `nova --buat-kunci <folder>`, `nova --rilis <pemasang> <url> <catatan>`.

## Pemasang melakukan apa

- Memasang per-pengguna di `%LOCALAPPDATA%\Programs\NOVA` (**tanpa admin**), ikon Start Menu, opsi ikon desktop.
- Opsi "Jalankan NOVA otomatis saat masuk Windows" (nilai registri `NOVA` yang sama dengan menu tray).
- Saat memperbarui: menutup NOVA yang berjalan, mengganti `nova.exe`, menyalakannya lagi.
- Saat menghapus: menutup NOVA, menghapus autostart; **data pengguna di `%APPDATA%\nova` dipertahankan** kecuali pengguna memilih menghapusnya.

## Data pengguna dan pembaruan

Memori, catatan, dan kunci terenkripsi ada di `%APPDATA%\nova`, terpisah dari program, sehingga pembaruan tidak menyentuhnya.
Skema database memakai `CREATE TABLE IF NOT EXISTS`; bila Anda mengubah skema di rilis berikutnya, tambahkan migrasi yang aman diulang.

## Keamanan pembaruan

| Ancaman | Penangkal |
|---|---|
| Akun GitHub/rilis disusupi, pemasang diganti | `latest.json` harus bertanda tangan **kunci privat yang tidak ada di GitHub-nya penyerang** (hanya sebagai secret Actions); tanpa itu, ditolak |
| Lalu lintas dibelokkan (MITM) | HTTPS + tanda tangan + SHA-256 pemasang di dalam pesan bertanda tangan |
| Dipaksa turun ke versi lama yang rentan | Hanya versi **lebih tinggi** diterima |
| Model/halaman web menyuruh NOVA memperbarui | Alat `pembaruan pasang` selalu minta persetujuan Bos |
| Unduhan raksasa/rusak | Batas 300 MB, file dibuang bila hash tidak cocok |

Batasan jujur: bila **kunci privat dan akun GitHub sama-sama bocor**, penyerang bisa merilis pembaruan yang sah di mata NOVA.
Lindungi akun dengan 2FA dan simpan kunci privat terpisah.

## Peringatan SmartScreen dan tanda tangan kode

Tanpa sertifikat **Authenticode**, Windows SmartScreen bisa menampilkan "Windows protected your PC" pada pemasang yang baru dirilis.
Itu tidak memengaruhi pembaruan otomatis (diverifikasi dengan tanda tangan Ed25519 di atas), tetapi memengaruhi unduhan manual pertama.
Pilihan menandatangani kode: sertifikat berbayar (OV/EV), **Azure Trusted Signing**, atau **SignPath Foundation** (gratis untuk proyek sumber terbuka).
Tambahkan langkah `signtool sign` di `release.yml` sebelum langkah "Terbitkan" bila sudah punya.

## Jalur distribusi lain (opsional)

- **winget**: kirim manifest ke `microsoft/winget-pkgs` menunjuk ke pemasang di Releases; pengguna lalu `winget upgrade NOVA`.
- **Microsoft Store / MSIX**: pembaruan dikelola Store, butuh akun pengembang dan paket MSIX.
