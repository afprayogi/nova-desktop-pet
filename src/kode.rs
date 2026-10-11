//! Pengetahuan bahasa pemrograman (ringkas, praktis). Diindeks oleh pencarian hibrida; sumber 'pemrograman'.
//! Ringkasan umum dari pengetahuan saya; untuk detail resmi, rujuk dokumentasi bahasa terkait.

pub const KB: &[(&str, &str)] = &[
    ("Rust: ownership dan borrowing", "Setiap nilai punya satu pemilik; saat pemilik keluar scope nilai di-drop. Meminjam dengan &T (banyak, baca) atau &mut T (satu, tulis) - tidak boleh bersamaan. Pindah (move) membuat variabel lama tak bisa dipakai; gunakan .clone() atau pinjam bila perlu."),
    ("Rust: error borrow checker umum", "E0382 'use of moved value': nilai sudah dipindah; pinjam (&) atau clone. E0499 'cannot borrow as mutable more than once': hanya satu &mut aktif; persempit scope atau pecah struct. E0502 'cannot borrow as mutable because also borrowed as immutable': selesaikan pemakaian & dulu. E0106 'missing lifetime specifier': tambahkan anotasi 'a atau kembalikan nilai bermilik (String)."),
    ("Rust: Option, Result, dan operator ?", "Option<T> = Some/None; Result<T,E> = Ok/Err. Operator ? meneruskan error ke pemanggil (fungsi harus mengembalikan Result/Option). Gunakan .unwrap_or, .map, .ok_or, .and_then; hindari unwrap() di kode produksi. anyhow untuk aplikasi, thiserror untuk pustaka."),
    ("Rust: trait, generic, dan lifetime", "Trait mendefinisikan perilaku (mirip interface); generic <T: Trait> memakai monomorfisasi. impl Trait untuk argumen/return, dyn Trait untuk objek dinamis (Box<dyn Trait>). Lifetime 'a menyatakan referensi hidup selama yang dirujuk; kompilator sering menyimpulkannya (elision)."),
    ("Rust: Cargo dan struktur proyek", "cargo new, cargo build --release, cargo run, cargo test, cargo check (cepat), cargo clippy (lint), cargo fmt. Cargo.toml: [dependencies], [features], [profile.release]. Modul: mod x; di main.rs memuat src/x.rs. Gunakan cargo add <crate>."),
    ("Rust: async dan tokio", "async fn mengembalikan Future yang baru berjalan saat .await. Perlu runtime (tokio): #[tokio::main] atau Runtime::new().block_on. spawn untuk tugas paralel, tokio::select! untuk balapan, mpsc untuk kanal. Jangan memanggil kode blocking di async; pakai spawn_blocking."),
    ("Rust: iterator dan closure", "iter().map().filter().collect() malas (lazy). into_iter memindahkan, iter meminjam, iter_mut meminjam mutable. Closure menangkap lingkungan: Fn, FnMut, FnOnce; kata kunci move memindahkan tangkapan. sum, fold, zip, enumerate, windows, chunks sering dipakai."),
    ("Python: dasar dan indentasi", "Blok ditentukan indentasi (4 spasi); campur tab/spasi memicu IndentationError/TabError. Tipe dinamis; list, dict, set, tuple. f-string: f'{x:.2f}'. Virtualenv: python -m venv .venv lalu aktifkan; pip install -r requirements.txt."),
    ("Python: error umum dan solusinya", "ModuleNotFoundError: pip install paket atau cek virtualenv aktif. KeyError: kunci dict tak ada, pakai .get(). TypeError 'NoneType' object is not subscriptable: fungsi mengembalikan None, periksa return. IndexError: indeks di luar rentang. AttributeError: objek tak punya atribut itu, cek tipe. UnicodeDecodeError: tentukan encoding='utf-8'."),
    ("Python: comprehension, generator, dan dekorator", "[x*x for x in data if x>0] list comprehension; {k:v for ...} dict; (x for x in ...) generator hemat memori. yield membuat generator. Dekorator @fungsi membungkus fungsi. with open(...) as f menjamin file tertutup. dataclass untuk struct sederhana."),
    ("Python: async dan paket populer", "asyncio: async def, await, asyncio.run(main()), gather. Paket: requests/httpx (HTTP), pandas dan numpy (data), matplotlib (grafik), flask/fastapi (web), pytest (uji), pyserial (serial), pymodbus (Modbus)."),
    ("JavaScript: var, let, const, dan scope", "var berscope fungsi dan ter-hoist; let/const berscope blok. const mencegah penugasan ulang, bukan mutasi objek. === membandingkan tanpa konversi tipe. Closure: fungsi mengingat variabel luar. this bergantung pemanggil; arrow function mewarisi this."),
    ("JavaScript: promise, async, dan error umum", "async/await membungkus Promise; tangkap dengan try/catch atau .catch. 'Cannot read properties of undefined': objek undefined sebelum mengakses properti; pakai optional chaining a?.b dan periksa data. 'x is not a function': salah nama/impor atau tipe. CORS error: server harus mengirim header Access-Control-Allow-Origin."),
    ("JavaScript dan TypeScript: npm dan tipe", "npm init, npm install, npm run. package.json mencatat dependensi. TypeScript menambah tipe statis: interface, type, generics; compile dengan tsc. Error 'Type X is not assignable to type Y': sesuaikan tipe atau gunakan union/narrowing."),
    ("C: pointer dan memori", "Pointer menyimpan alamat: int *p = &x; *p mengakses nilai. malloc/free manual; lupa free = kebocoran, free dua kali = crash. Segmentation fault: akses memori tak valid (pointer NULL, buffer overflow, use-after-free). Array tidak diperiksa batasnya; gunakan ukuran eksplisit."),
    ("C dan C++: error kompilasi dan linker", "'undefined reference to': fungsi dideklarasikan tapi tak ditautkan; tambahkan file .c/.cpp atau pustaka -l. 'implicit declaration': lupa #include. 'expected ; before': titik koma hilang di baris sebelumnya. Header guard (#pragma once) mencegah redefinisi."),
    ("C++: RAII, smart pointer, dan STL", "RAII: sumber daya dilepas di destruktor. unique_ptr (satu pemilik), shared_ptr (hitung referensi), hindari new/delete mentah. vector, map, unordered_map, string, algorithm (sort, find). Referensi const& menghindari salinan. Build: CMake atau Makefile."),
    ("Arduino dan mikrokontroler", "Arduino C++: setup() sekali, loop() berulang. pinMode, digitalWrite, analogRead (ADC 10-bit 0-1023), analogWrite (PWM), Serial.begin/println. Hindari delay() panjang, pakai millis(). Variabel yang diubah di interrupt harus volatile; ISR singkat. MicroPython: machine.Pin, machine.ADC."),
    ("C#: dasar dan error umum", "C# di .NET: class, property, LINQ (Where, Select, ToList), async/await Task. NullReferenceException: objek null diakses; gunakan null-check dan ?. . InvalidCastException: cast tipe salah. dotnet new, dotnet build, dotnet run. Windows Forms/WPF untuk GUI."),
    ("Java: dasar dan error umum", "Berorientasi objek; class, interface, generics, koleksi (List, Map). NullPointerException: referensi null diakses; periksa null atau Optional. ClassNotFoundException: classpath salah. Build dengan Maven/Gradle; JVM menjalankan bytecode. try-with-resources menutup sumber daya."),
    ("Go: goroutine, channel, dan error", "go f() menjalankan goroutine; channel (chan T) untuk komunikasi, sync.WaitGroup untuk menunggu. Error adalah nilai: if err != nil { return err }. nil pointer dereference: struktur belum diinisialisasi. go mod init, go run, go build; gofmt memformat."),
    ("SQL: SELECT, JOIN, dan indeks", "SELECT kolom FROM tabel WHERE kondisi GROUP BY ... HAVING ... ORDER BY ... LIMIT. JOIN: INNER (irisan), LEFT (semua kiri). Indeks mempercepat WHERE/JOIN pada kolom terindeks. Gunakan query berparameter, jangan menyambung string (SQL injection). SQLite untuk basis data lokal."),
    ("Bash dan PowerShell", "Bash: pipa |, redirect >, variabel $VAR, for/if, $(cmd). PowerShell: objek mengalir di pipa (Get-ChildItem | Where-Object), variabel $x, Set-ExecutionPolicy RemoteSigned bila skrip diblokir ('running scripts is disabled'). Kutip: tanda ' literal, \" mengembang variabel."),
    ("HTML dan CSS dasar", "HTML: struktur (div, p, a, img, form). CSS: selektor, box model (margin, border, padding), Flexbox (display:flex; justify-content; align-items) untuk satu dimensi, Grid (display:grid; grid-template-columns) untuk dua dimensi. @media untuk responsif."),
    ("Git: alur kerja dasar", "git init/clone; git status; git add; git commit -m; git push/pull; git branch dan git switch -c fitur; git merge atau git rebase. .gitignore mengabaikan file (target/, node_modules/). git log --oneline untuk riwayat."),
    ("Git: membatalkan dan menyelesaikan konflik", "Batalkan perubahan file: git restore <file>. Batalkan commit terakhir (pertahankan perubahan): git reset --soft HEAD~1. Buat commit pembatal: git revert <hash>. Konflik merge: edit penanda <<<<<<< ======= >>>>>>>, lalu git add dan git commit. Simpan sementara: git stash."),
    ("Algoritma: kompleksitas Big-O", "O(1) konstan, O(log n) pencarian biner, O(n) satu lintasan, O(n log n) sort efisien, O(n^2) dua loop bersarang, O(2^n) eksponensial. Pilih struktur data: array (akses indeks), hash map (cari rata-rata O(1)), tumpukan/antrean, heap (prioritas), pohon (hierarki)."),
    ("Algoritma: pencarian, sorting, dan graf", "Binary search pada data terurut O(log n). Quick/merge sort O(n log n). BFS (antrean) mencari jalur terpendek tak berbobot; DFS (tumpukan/rekursi) menjelajah dalam. Dijkstra untuk bobot nonnegatif. Dynamic programming menyimpan hasil submasalah (memoization)."),
    ("Rekursi dan dynamic programming", "Rekursi butuh kasus dasar dan langkah yang mengecil; tanpa itu terjadi stack overflow. Fibonacci naif O(2^n), dengan memoization O(n). DP: definisikan state, transisi, dan urutan pengisian; contoh knapsack, LCS, coin change."),
    ("Debugging: cara sistematis", "1) Reproduksi masalah dengan langkah pasti. 2) Kecilkan menjadi contoh minimal. 3) Baca pesan error dan stack trace dari baris paling relevan (kode Anda). 4) Pasang log atau gunakan debugger (breakpoint, watch). 5) Bagi dua (bisect) perubahan/kode untuk menemukan penyebab. 6) Perbaiki akar masalah dan tambahkan tes."),
    ("Regex dasar", "^ awal, $ akhir, . satu karakter, * nol atau lebih, + satu atau lebih, ? opsional, [a-z] kelas, \\d digit, \\w huruf-angka, (grup), | atau, {n,m} jumlah. Serakah vs malas (*?). Uji di regex101 sebelum dipakai."),
    ("HTTP, REST, dan JSON", "Metode: GET baca, POST buat, PUT/PATCH ubah, DELETE hapus. Status: 200 OK, 201 Created, 400 request salah, 401 belum autentikasi, 403 dilarang, 404 tidak ada, 429 terlalu banyak, 500 error server. JSON: objek {}, array [], string/angka/boolean/null; header Content-Type: application/json."),
    ("Docker dan kontainer", "docker build -t nama ., docker run -p 8080:80 nama, docker ps, docker logs, docker compose up -d. Image berisi aplikasi + dependensi; kontainer adalah instance berjalan. Volume menyimpan data agar tidak hilang."),
    ("PLC: Structured Text (ST)", "ST mirip Pascal: IF cond THEN ... ELSIF ... END_IF; FOR i := 0 TO 9 DO ... END_FOR; CASE x OF 1: ... END_CASE; penugasan :=; tipe BOOL, INT, REAL, TIME, TON (timer on-delay) dan CTU (penghitung naik). Contoh seal-in: Motor := (Start OR Motor) AND NOT Stop;"),
    ("PLC dan embedded: pola pemrograman", "Gunakan state machine (CASE state) untuk urutan proses; debounce input dengan timer; skala sinyal analog 4-20 mA ke satuan teknik; jangan memblokir scan cycle dengan loop panjang; beri interlock dan emergency stop dengan logika fail-safe (NC). Dokumentasikan alamat I/O."),
    ("Modbus untuk pemrogram", "Alamat register: coil 0xxxx (bit baca/tulis), discrete input 1xxxx, input register 3xxxx, holding register 4xxxx. Fungsi: 01/02/03/04 baca, 05/06 tulis tunggal, 15/16 tulis banyak. RTU: serial RS-485 dengan baudrate/parity/slave ID; TCP: port 502. Float 32-bit memakai dua register (perhatikan urutan byte)."),
    ("Praktik kode bersih", "Nama jelas, fungsi kecil satu tugas, hindari duplikasi (DRY), tangani error eksplisit, tulis tes untuk perilaku penting, komentar menjelaskan alasan bukan apa. Review kode dan format otomatis (formatter, linter) menjaga konsistensi."),
];

pub const EDGES: &[(&str, &str, &str)] = &[
    ("Rust: ownership dan borrowing", "menyebabkan", "Rust: error borrow checker umum"),
    ("Rust: ownership dan borrowing", "mendasari", "Rust: trait, generic, dan lifetime"),
    ("Rust: Option, Result, dan operator ?", "bagian_dari", "Rust: error borrow checker umum"),
    ("Rust: async dan tokio", "memakai", "Rust: iterator dan closure"),
    ("Rust: Cargo dan struktur proyek", "mengelola", "Rust: async dan tokio"),
    ("Python: dasar dan indentasi", "menyebabkan", "Python: error umum dan solusinya"),
    ("Python: comprehension, generator, dan dekorator", "memperluas", "Python: dasar dan indentasi"),
    ("Python: async dan paket populer", "memperluas", "Python: comprehension, generator, dan dekorator"),
    ("JavaScript: var, let, const, dan scope", "mendasari", "JavaScript: promise, async, dan error umum"),
    ("JavaScript dan TypeScript: npm dan tipe", "memperluas", "JavaScript: var, let, const, dan scope"),
    ("C: pointer dan memori", "menyebabkan", "C dan C++: error kompilasi dan linker"),
    ("C++: RAII, smart pointer, dan STL", "memperbaiki", "C: pointer dan memori"),
    ("Arduino dan mikrokontroler", "memakai", "C: pointer dan memori"),
    ("Debugging: cara sistematis", "dipakai_pada", "Python: error umum dan solusinya"),
    ("Debugging: cara sistematis", "dipakai_pada", "Rust: error borrow checker umum"),
    ("Git: alur kerja dasar", "dilengkapi", "Git: membatalkan dan menyelesaikan konflik"),
    ("Algoritma: kompleksitas Big-O", "mendasari", "Algoritma: pencarian, sorting, dan graf"),
    ("Rekursi dan dynamic programming", "bagian_dari", "Algoritma: pencarian, sorting, dan graf"),
    ("PLC: Structured Text (ST)", "terkait", "PLC dan embedded: pola pemrograman"),
    ("Modbus untuk pemrogram", "dipakai_pada", "PLC dan embedded: pola pemrograman"),
    ("HTTP, REST, dan JSON", "dipakai_pada", "JavaScript: promise, async, dan error umum"),
    ("Praktik kode bersih", "mendukung", "Debugging: cara sistematis"),
    ("SQL: SELECT, JOIN, dan indeks", "terkait", "HTTP, REST, dan JSON"),
];

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn edges_reference_existing_titles() {
        for (a, _, b) in EDGES {
            assert!(KB.iter().any(|k| k.0 == *a), "{a}");
            assert!(KB.iter().any(|k| k.0 == *b), "{b}");
        }
    }
}
