//! Perintah /garis-miring (`slash`).
use super::*;

impl Engine {
    pub(super) async fn slash(&mut self, rest: &str) {
        let (cmd, arg) = rest.split_once(' ').map(|(a, b)| (a, b.trim())).unwrap_or((rest, ""));
        let kind = match cmd {
            "makro" | "scaffold" | "killport" => "palu",
            "email" | "google" | "outlook" => "email",
            "graf" | "graphify" => "graf",
            "berkas" | "pilih" => "baca",
            "bahasa" | "proyek" | "log" => "baca",
            "port" | "iklan" | "bec" => "cek",
            "meme" => "tawa",
            _ => "",
        };
        self.act(kind, "", 2800);
        match cmd.to_lowercase().as_str() {
            "bantuan" | "help" | "?" => {
                if arg.is_empty() {
                    self.tx.sys(HELP)
                } else {
                    self.tx.sys(crate::cli::find(arg))
                }
            }
            "ollama" => {
                let mut c = self.cfg.clone();
                c.base_url = "http://localhost:11434/v1".into();
                c.api_key = "ollama".into();
                self.apply_cfg(c);
                self.tx.sys("Terhubung ke Ollama lokal (http://localhost:11434/v1). Memindai model... (pasang model dulu, mis. 'ollama pull qwen2.5:1.5b')");
                self.scan().await;
            }
            "url" if !arg.is_empty() => {
                let mut c = self.cfg.clone();
                c.base_url = arg.to_string();
                self.apply_cfg(c);
                self.tx.sys("URL model disimpan. Ketik /scan untuk memindai model.");
            }
            "key" if !arg.is_empty() => {
                let mut c = self.cfg.clone();
                c.api_key = arg.to_string();
                self.apply_cfg(c);
                self.tx.sys("API key disimpan (tidak ditampilkan).");
            }
            "aksi" => {
                // /aksi <jenis> [teks]  atau  /aksi demo : pratinjau animasi peliharaan
                const KINDS: &[&str] = &["tembak", "cari", "sapu", "pikir", "baca", "email", "koran", "alarm", "ketik", "baterai", "palu", "cek", "graf", "tawa"];
                if arg == "demo" {
                    let tx = self.tx.clone();
                    tokio::spawn(async move {
                        for k in KINDS {
                            tx.send(Event::Act { kind: (*k).into(), text: if *k == "tembak" { "ads.doubleclick.net".into() } else { String::new() }, ms: 2800 });
                            tokio::time::sleep(std::time::Duration::from_millis(3200)).await;
                        }
                    });
                    self.tx.sys(format!("🎬 Demo animasi: {}", KINDS.join(", ")));
                } else if arg.is_empty() {
                    self.tx.sys(format!("Pakai: /aksi <jenis> [teks] atau /aksi demo. Jenis: {}", KINDS.join(", ")));
                } else {
                    let (k, t) = arg.split_once(' ').unwrap_or((arg, ""));
                    self.act(k, t, 2800);
                }
            }
            "indeks" => {
                if arg == "bangun" {
                    std::thread::spawn(|| {
                        crate::indeks::rebuild();
                    });
                    self.tx.sys("🗂 Membangun ulang indeks berkas di latar (beberapa menit, ringan di CPU).");
                } else {
                    self.agent_say(3, &crate::indeks::status());
                }
            }
            "llm" => self.llm_cmd(arg).await,
            "mulai" => self.agent_say(0, GUIDE),
            "otomatis" => {
                let r = tokio::task::spawn_blocking({
                    let a = arg.to_string();
                    move || crate::sistem::auto_cmd(&a)
                })
                .await
                .unwrap_or_default();
                self.agent_say(0, &r);
            }
            "sholat" | "salat" | "shalat" => {
                let r = crate::sholat::cmd(&self.http, arg).await;
                self.agent_say(3, &r);
            }
            "semantik" => {
                match crate::semantik::scores(&self.http, arg).await {
                    Ok(v) => self.tx.sys(format!("Skor semantik: {}", v.iter().take(4).map(|(l, s)| format!("{l} {s:.2}")).collect::<Vec<_>>().join(" · "))),
                    Err(e) => self.tx.sys(format!("Semantik gagal: {e}")),
                }
            }
            "cadangan" => {
                let n = tokio::task::spawn_blocking(crate::cadangan::run).await.unwrap_or(0);
                self.agent_say(0, &if n > 0 { format!("💾 Cadangan selesai ({n} berkas). Otomatis tiap hari, 7 hari terakhir disimpan (D:/NOVA-backup bila ada drive D).") } else { "💾 Cadangan hari ini sudah ada (otomatis tiap hari, disimpan 7 hari).".into() });
            }
            "build" => {
                let (tx, http) = (self.tx.clone(), self.http.clone());
                let arg = arg.to_string();
                self.act("palu", "", 6000);
                self.tx.sys("🔨 Build dimulai di latar…");
                tokio::spawn(async move {
                    match crate::bangun::run(&arg).await {
                        Ok((true, s, secs)) => {
                            crate::notify::toast("NOVA - build selesai", &format!("Berhasil dalam {secs} detik"));
                            crate::makan::catat("build");
                            if let Some(m) = crate::buddy::add_xp("build") {
                                tx.notice(m);
                            }
                            let (u, _) = crate::buddy::amati("build berhasil selesai");
                            if !u.is_empty() {
                                tx.notice(format!("{}: {u}", crate::buddy::get().nama));
                            }
                            tx.send(Event::Mood { emo: "gembira".into(), level: 0.9 });
                            tx.notice(format!("✅ Build selesai ({secs} dtk): {s}"));
                            tx.send(Event::Chat(ChatLine { who: Who::Agent(1), text: format!("✅ Build selesai dalam {secs} detik 🎉\n{s}"), media: Vec::new() }));
                        }
                        Ok((false, s, secs)) => {
                            crate::notify::toast("NOVA - build gagal", "Ada error pada build");
                            tx.send(Event::Chat(ChatLine { who: Who::Agent(1), text: format!("❌ Build gagal ({secs} dtk). Error utama:\n{s}"), media: Vec::new() }));
                            tx.send(Event::Ask { question: "Mau kubantu cek error ini?".into(), options: vec!["Jelaskan error".into(), "Tutup".into()] });
                        }
                        Err(e) => tx.sys(format!("🔨 {e}")),
                    }
                    let _ = http;
                });
            }
            "paksa" => {
                let r = crate::hewan::paksa_cmd(arg);
                self.agent_say(2, &r);
            }
            "aplikasi" | "apps" => {
                let r = tokio::task::spawn_blocking({ let a = arg.to_string(); move || crate::aplikasi::daftar_teks(&a) }).await.unwrap_or_default();
                self.agent_say(0, &r)
            }
            "internet" | "jaringan" => {
                self.agent_say(0, "Memeriksa jalur internet (±15 detik)…");
                let r = crate::internet::laporan(&self.http).await;
                self.agent_say(0, &r)
            }
            "riwayat" => {
                let r = match arg.to_lowercase().as_str() {
                    "on" => crate::riwayat::set(true),
                    "off" => crate::riwayat::set(false),
                    _ => tokio::task::spawn_blocking(crate::riwayat::ringkas).await.unwrap_or_default(),
                };
                self.agent_say(0, &r)
            }
            "kru" => {
                if arg.is_empty() {
                    self.agent_say(0, "Pakai: /kru <tujuan>. Kru (peneliti, pelaksana, penulis) mengerjakan tugas berurutan dengan alat aman; yang butuh persetujuan dilaporkan, bukan dijalankan.")
                } else {
                    self.kru(arg).await
                }
            }
            "otak" | "ingatan" => {
                let r = if arg.eq_ignore_ascii_case("belajar") {
                    tokio::task::spawn_blocking(|| crate::otak::harian(true)).await.ok().flatten().unwrap_or_default()
                } else {
                    tokio::task::spawn_blocking(crate::otak::teks).await.unwrap_or_default()
                };
                self.agent_say(0, &r)
            }
            "paket" | "wireshark" => {
                let r = if arg.to_lowercase().contains("tangkap") {
                    self.agent_say(0, "Menangkap paket ±15 detik. Setujui kotak UAC Windows bila muncul…");
                    crate::paket::tangkap(15).await
                } else {
                    crate::paket::teks_monitor(&crate::paket::monitor().await)
                };
                self.agent_say(0, &r)
            }
            "panelmusik" => {
                let r = crate::musik::set_panel(!arg.eq_ignore_ascii_case("off") && arg != "0");
                self.agent_say(0, &r)
            }
            "musikjaga" => {
                let r = crate::musik::set_jaga(!arg.eq_ignore_ascii_case("off") && arg != "0");
                self.agent_say(0, &r)
            }
            "musikotomatis" => {
                let r = crate::musik::set_auto(!arg.eq_ignore_ascii_case("off") && arg != "0");
                self.agent_say(0, &r)
            }
            "smtp" => {
                let p: Vec<&str> = arg.split_whitespace().collect();
                let r = if p.len() >= 3 { crate::surel::setup(p[0], p[1], &p[2..].join(" ")) } else { crate::surel::setup("", "", "") };
                self.tx.sys(r)
            }
            "pencapaian" | "achievement" => self.agent_say(0, &crate::makan::pencapaian()),
            "agen" | "makan" => self.agent_say(0, &crate::makan::status_agen()),
            "suara" => {
                let on = !matches!(arg.trim(), "off" | "mati");
                crate::makan::set_suara(on);
                if on {
                    crate::generator::bikin_suara();
                    crate::makan::sfx("blip");
                }
                self.agent_say(0, if on { "Efek suara aktif 🔊 (naik level, pencapaian, error)." } else { "Efek suara dimatikan 🔇" });
            }
            "karakter" | "tampil" => {
                let a = arg.trim();
                let r = if let Some(p) = a.strip_prefix("buat ") {
                    let (nama, gambar) = p.trim().split_once(' ').unwrap_or((p.trim(), ""));
                    let (n, g) = (nama.to_string(), gambar.to_string());
                    tokio::task::spawn_blocking(move || crate::generator::buat(&n, &g)).await.unwrap_or_default()
                } else if let Some(p) = a.strip_prefix("pasang ") {
                    crate::karakter::pasang(p)
                } else if a.is_empty() {
                    crate::karakter::laporan()
                } else {
                    crate::karakter::set_aktif(a).unwrap_or_else(|e| e)
                };
                self.agent_say(0, &r);
            }
            "buddy" | "sahabat" => {
                let (txt, lv) = crate::buddy::cmd(arg);
                self.agent_say(0, &txt);
                if let Some(m) = lv {
                    self.tx.sys(m);
                    self.tx.send(Event::Mood { emo: "gembira".into(), level: 1.0 });
                }
            }
            "ingat" | "fakta" => self.agent_say(3, &crate::fakta::daftar()),
            "lupa" => {
                let r = crate::fakta::lupa(arg);
                self.agent_say(3, &r);
            }
            "persona" | "kepribadian" => {
                let a = arg.to_string();
                let r = tokio::task::spawn_blocking(move || crate::persona::cmd(&a)).await.unwrap_or_default();
                self.agent_say(0, &r);
            }
            "analitik" | "ramalan" | "anomali" => {
                let r = tokio::task::spawn_blocking(crate::analitik::laporan).await.unwrap_or_default();
                self.agent_say(2, &r);
            }
            "lokal" => {
                let r = crate::lokal::status(&self.http).await;
                self.agent_say(0, &r);
            }
            "omni" | "omniroute" => {
                let a = arg.trim();
                if a == "pindai" || a == "scan" {
                    match crate::omniroute::scan(&self.http).await {
                        Ok(m) => self.agent_say(0, &format!("🛰 {} model teruji. Terbaik: {}", m.len(), m.iter().take(5).map(|x| format!("{} ({} ms)", x.model, x.ms)).collect::<Vec<_>>().join(", "))),
                        Err(e) => self.agent_say(0, &format!("🛰 Gagal memindai: {e}")),
                    }
                } else {
                    self.agent_say(0, &crate::omniroute::laporan());
                }
            }
            "latih" => {
                let r = tokio::task::spawn_blocking(crate::belajar::retrain).await.unwrap_or_default();
                self.agent_say(0, &format!("🧠 {r}"));
            }
            "pola" | "kelelahan" => {
                let r = tokio::task::spawn_blocking(crate::hewan::laporan).await.unwrap_or_default();
                self.agent_say(3, &r);
            }
            "jadwal" | "tidur" => {
                let r = tokio::task::spawn_blocking(crate::rencana::plan).await.unwrap_or_default();
                self.agent_say(3, &r);
            }
            "diskusi" => self.diskusi(arg.to_string()).await,
            "berkas" => {
                let path = arg.trim_matches('"').to_string();
                let (txt, opts) = tokio::task::spawn_blocking(move || crate::seret::info(&path)).await.unwrap_or_else(|_| ("gagal membaca info".into(), vec![]));
                self.act("baca", "", 2500);
                self.agent_say(0, &txt);
                if !opts.is_empty() {
                    self.tx.send(Event::Ask { question: "Mau diapakan?".into(), options: opts });
                }
            }
            "pilih" => {
                let folder = arg.starts_with("folder");
                self.tx.sys(if folder { "📁 Memilih folder…" } else { "📎 Memilih berkas…" });
                match tokio::task::spawn_blocking(move || crate::seret::pick(folder)).await.ok().flatten() {
                    Some(p) => return Box::pin(self.slash(&format!("berkas {p}"))).await,
                    None => self.tx.sys("Pemilihan dibatalkan."),
                }
            }
            "gemini" => {
                let (sub, rest) = arg.split_once(' ').map(|(a, b)| (a, b.trim())).unwrap_or((arg, ""));
                match sub {
                    "key" => {
                        if crate::gemini::set_key(rest) {
                            crate::gemini::set_enabled(true);
                            self.tx.sys("🔑 Key Gemini tersimpan terenkripsi (DPAPI). Memilih model gratis yang bisa dipakai…");
                            match crate::gemini::auto_setup(&self.http).await {
                                Ok(m) => self.agent_say(0, &format!("☁ Gemini AKTIF dengan model {m} (dipilih otomatis; berpindah sendiri bila kuota habis). Matikan: /gemini off")),
                                Err(e) => self.tx.sys(format!("☁ Key tersimpan, tetapi belum ada model yang bisa dipakai: {e}")),
                            }
                        } else {
                            self.tx.sys("Key tidak valid. Buat di aistudio.google.com/apikey lalu: /gemini key <kunci>");
                        }
                    }
                    "on" if crate::gemini::has_key() => {
                        crate::gemini::set_enabled(true);
                        self.tx.sys("☁ Gemini aktif.");
                    }
                    "off" => {
                        crate::gemini::set_enabled(false);
                        self.tx.sys("☁ Gemini dimatikan; kembali ke mode offline/Llama.");
                    }
                    "models" | "model" if rest.is_empty() || rest == "auto" => {
                        match crate::gemini::list_models(&self.http).await {
                            Ok(all) => {
                                let ranked = crate::gemini::rank_models(&all);
                                if rest == "auto" || sub == "model" {
                                    match crate::gemini::auto_setup(&self.http).await {
                                        Ok(m) => self.tx.sys(format!("☁ Model Gemini otomatis: {m} (sudah dicoba dan berjalan).")),
                                        Err(e) => self.tx.sys(format!("☁ Gagal memilih model otomatis: {e}")),
                                    }
                                } else {
                                    let cur = crate::gemini::model();
                                    self.tx.sys(format!("☁ Model flash tersedia ({} dari {} model):\n{}\nAktif: {cur}. Pilih: /gemini model <nama> atau /gemini model auto", ranked.len(), all.len(), ranked.iter().take(12).map(|m| format!("{} {m}", if *m == cur { "▶" } else { "•" })).collect::<Vec<_>>().join("\n")));
                                }
                            }
                            Err(e) => self.tx.sys(format!("☁ Gagal mengambil daftar model: {e}")),
                        }
                    }
                    "model" if !rest.is_empty() => {
                        crate::gemini::set_model(rest);
                        self.tx.sys(format!("☁ Model Gemini: {rest}"));
                    }
                    "status" => {
                        let st = if !crate::gemini::ready() { "tidak aktif (mati/tanpa key)" } else if crate::gemini::degraded() { "OFFLINE sementara karena gangguan — memakai Llama lokal, dicoba lagi otomatis" } else { "ONLINE" };
                        self.tx.sys(format!("☁ Status Gemini: {st}. Model: {}. {}", crate::gemini::model(), crate::lokal::status(&self.http).await));
                    }
                    "cache" => {
                        if rest == "hapus" {
                            self.tx.sys(format!("💾 Memori Gemini dikosongkan ({} entri).", crate::gemini::cache_clear()));
                        } else {
                            let (n, hits) = crate::gemini::cache_stats();
                            self.tx.sys(format!("💾 Memori Gemini: {n} entri · {hits} kali dipakai ulang (tiap pemakaian ulang = 1 panggilan/token hemat). Kosongkan: /gemini cache hapus"));
                        }
                    }
                    "tes" => {
                        let r = crate::gemini::generate(&self.http, "Balas satu kata.", "Katakan: siap", false, false).await;
                        self.tx.sys(match r {
                            Ok(t) => format!("☁ Gemini ({}) merespons: {}", crate::gemini::model(), t),
                            Err(e) => format!("☁ Gemini gagal: {e}"),
                        });
                    }
                    _ => self.tx.sys(format!(
                        "☁ Gemini: {} · model {} · key {}.\nPerintah: /gemini key <kunci> | on | off | model <nama> | tes. Dikirim ke Google hanya teks perintah + jam/mode; isi file/layar/email tidak.",
                        if crate::gemini::ready() { "AKTIF" } else { "mati" },
                        crate::gemini::model(),
                        if crate::gemini::has_key() { "tersimpan" } else { "belum ada" }
                    )),
                }
            }
            "cerewet" => {
                if let Ok(n) = arg.parse::<u8>() {
                    crate::adegan::set_level(n.min(2));
                }
                self.tx.sys(format!("🔇 Tingkat bicara NOVA: {} (0 = hanya yang penting, 1 = normal, 2 = ramai). Ubah: /cerewet 0|1|2", crate::adegan::level()));
            }
            "scan" => self.scan().await,
            "iklan" => match arg {
                "on" => self.ads_set(true).await,
                "off" => self.ads_set(false).await,
                "ekstensi" => {
                    let tok = std::fs::read_to_string(crate::api::token_path()).unwrap_or_default();
                    match crate::ekstensi::generate(crate::port(), tok.trim()) {
                        Ok((d, n)) => {
                            let _ = std::process::Command::new("explorer").arg(&d).spawn();
                            self.agent_say(2, &format!("🧩 Ekstensi dibuat ({n} domain) di {}.\nPasang sekali: buka edge://extensions → aktifkan Mode pengembang → 'Muat yang belum dipaketkan' → pilih folder itu. Iklan pop-up/dalam halaman lalu ikut diblokir dan peliharaan menembaknya. Setelah /iklan update, ketik /iklan ekstensi lagi lalu tekan Muat ulang di halaman ekstensi.", d.display()));
                        }
                        Err(e) => self.tx.sys(format!("Gagal membuat ekstensi: {e}")),
                    }
                }
                "update" => {
                    self.tx.sys("Mengunduh daftar iklan ke disk...");
                    match crate::adblock::update(&self.http).await {
                        Ok(n) => self.tx.sys(format!("Daftar iklan diperbarui: {n} domain.")),
                        Err(e) => self.tx.sys(format!("Gagal memperbarui daftar: {e}")),
                    }
                }
                _ => self.tx.sys(format!(
                    "🛡 Pemblokir iklan: {} · diblokir {} dari {} query · daftar {} domain. Perintah: /iklan on|off|update|ekstensi (ekstensi = blokir iklan di dalam halaman web). Setelah on, setel DNS Windows ke 127.0.0.1 agar berlaku.",
                    if crate::adblock::ON.load(std::sync::atomic::Ordering::Relaxed) { "AKTIF" } else { "mati" },
                    crate::adblock::BLOCKED.load(std::sync::atomic::Ordering::Relaxed),
                    crate::adblock::TOTAL.load(std::sync::atomic::Ordering::Relaxed),
                    crate::adblock::LISTED.load(std::sync::atomic::Ordering::Relaxed)
                )),
            },
            "graf" => match arg {
                "" => self.agent_say(3, &crate::graf::stats()),
                "lihat" => match crate::graf::export_html() {
                    Some(p) => {
                        crate::google::open_in_browser(&p.to_string_lossy());
                        self.agent_say(3, "🕸 Peta graf ingatan dibuka di browser (berkas lokal, tanpa server).");
                    }
                    None => self.agent_say(3, "Graf masih kosong; ngobrol atau /graphify <folder> dulu."),
                },
                t => {
                    let t = t.to_string();
                    let r = tokio::task::spawn_blocking(move || crate::graf::about(&t)).await.unwrap_or_default();
                    self.agent_say(3, &r);
                }
            },
            "graphify" => {
                let p = std::path::PathBuf::from(arg.trim_matches('"'));
                if p.is_dir() {
                    let r = tokio::task::spawn_blocking(move || crate::graf::graphify_folder(&p)).await.unwrap_or_default();
                    self.agent_say(3, &r);
                } else {
                    self.tx.sys("Pakai: /graphify C:/kode/proyek");
                }
            }
            "bec" => match arg {
                "on" => {
                    crate::bec::set_enabled(true);
                    self.agent_say(3, "Tes BEC harian AKTIF 📘. Tiap malam (jam di jadwal.json, bawaan 22:00) layar peliharaan penuh-layar dan harus diselesaikan dulu. Ada tunda 10 menit (2x/malam) dan tombol darurat. Matikan: /bec off.");
                }
                "off" => {
                    crate::bec::set_enabled(false);
                    self.agent_say(3, "Tes BEC harian dimatikan.");
                }
                "sekarang" | "now" => {
                    crate::bec::force_now();
                    self.agent_say(3, "Baik, tes BEC dimulai sekarang. Peliharaan akan layar-penuh sebentar lagi.");
                }
                _ => self.agent_say(3, &crate::bec::report()),
            },
            "meme" => self.agent_say(1, &crate::meme::meme(arg, self.turns + arg.len())),
            "urungkan" => self.agent_say(0, &crate::berkas::urungkan()),
            "suka" => self.feedback(true),
            "tidak" => self.feedback(false),
            "diam" => {
                let m: u64 = arg.parse().unwrap_or(30);
                crate::bandit::set_quiet(m);
                self.tx.sys(format!("🤫 Aku diam {m} menit (hanya peringatan penting)."));
            }
            "minat" => self.tx.sys(crate::berita::set_interests(arg)),
            "proyek" => self.tx.sys(crate::proyek::proyek_cmd(arg)),
            "log" => self.tx.sys(crate::proyek::log_cmd(arg)),
            "makro" => {
                self.tx.send(Event::Thinking(Some(1)));
                let r = crate::makro::run(arg).await;
                self.tx.send(Event::Thinking(None));
                self.agent_say(1, &research::truncate(&r, 1800));
            }
            "port" => self.agent_say(2, &crate::makro::ports()),
            "killport" => self.agent_say(2, &crate::makro::killport(arg)),
            "scaffold" => self.agent_say(1, &crate::makro::scaffold(arg)),
            "bahasa" => {
                if let Some(l) = arg.strip_prefix("belajar ") {
                    self.tx.sys(format!("📚 Mengunduh paket '{}' dari GitHub tepercaya...", l.trim()));
                    match crate::bahasa::learn(&self.http, l.trim()).await {
                        Ok(n) => self.agent_say(1, &format!("Selesai: {n} potongan catatan {} masuk ke ingatanku. Tanya apa saja.", l.trim())),
                        Err(e) => self.tx.sys(format!("📚 Gagal: {e}")),
                    }
                } else {
                    let _ = tokio::task::spawn_blocking(crate::bahasa::detect).await;
                    self.agent_say(1, &format!("Terpasang di laptop ini: {}. Bahasa baru yang kamu pasang akan kupelajari otomatis (paket dari GitHub learnxinyminutes). Manual: /bahasa belajar <nama>.", crate::bahasa::summary()));
                }
            }
            "stop" => {
                self.mission = None;
                self.tx.send(Event::Approval(None));
                self.tx.sys("Misi dihentikan.");
            }
            "google" => {
                if let Some(who) = arg.strip_prefix("putus") {
                    self.tx.sys(crate::google::disconnect(who.trim()));
                } else if arg == "akun" {
                    let e = crate::google::emails();
                    self.tx.sys(if e.is_empty() { "🔑 Belum ada akun Google. Ketik /google untuk menambah.".to_string() } else { format!("🔑 Akun Google: {}", e.join(", ")) });
                } else {
                    // /google (atau /google tambah): buka browser untuk memilih akun; bisa diulang untuk banyak akun
                    tokio::spawn(crate::google::connect(self.tx.clone(), self.http.clone()));
                }
            }
            "outlook" => {
                if let Some(who) = arg.strip_prefix("putus") {
                    self.tx.sys(crate::outlook::disconnect(who.trim()));
                } else if arg == "akun" {
                    let e = crate::outlook::emails();
                    self.tx.sys(if e.is_empty() { "🔑 Belum ada akun Outlook. Ketik /outlook untuk menambah.".to_string() } else { format!("🔑 Akun Outlook: {}", e.join(", ")) });
                } else {
                    tokio::spawn(crate::outlook::connect(self.tx.clone(), self.http.clone()));
                }
            }
            "email" => {
                let (sub, rest) = arg.split_once(' ').map(|(a, b)| (a, b.trim())).unwrap_or((arg, ""));
                match sub {
                    "imap" => {
                        // /email imap <host|auto> <alamat> <sandi-aplikasi>  (sandi tidak ditampilkan)
                        let p: Vec<&str> = rest.split_whitespace().collect();
                        if p.len() >= 3 {
                            self.tx.sys(crate::imap::add(p[0], p[1], &p[2..].join(" ")));
                        } else {
                            self.tx.sys(crate::imap::add("", "", ""));
                        }
                    }
                    "hapus" => {
                        let r = if crate::google::emails().iter().any(|e| e == rest) { crate::google::disconnect(rest) } else if crate::outlook::emails().iter().any(|e| e == rest) { crate::outlook::disconnect(rest) } else { crate::imap::remove(rest) };
                        self.tx.sys(r);
                    }
                    "buka" => self.tx.sys(crate::google::open_inbox(rest)),
                    _ => {
                        self.tx.send(Event::Thinking(Some(3)));
                        let s = crate::google::summary(&self.http).await;
                        self.tx.send(Event::Thinking(None));
                        self.agent_say(3, &s);
                    }
                }
            }
            "ocr" => match arg {
                "on" => {
                    crate::layar::set_enabled(true);
                    self.tx.sys("🖥 Kesadaran layar AKTIF (OCR lokal tiap 3 menit, hanya kata kunci di RAM).");
                }
                "off" => {
                    crate::layar::set_enabled(false);
                    self.tx.sys("🖥 Kesadaran layar dimatikan.");
                }
                _ => self.tx.sys(format!("🖥 Kesadaran layar: {}. Pakai '/ocr on' atau '/ocr off'.", if crate::layar::enabled() { "aktif" } else { "mati" })),
            },
            _ => self.tx.sys("Perintah tidak dikenal. Ketik /bantuan."),
        }
    }

}
