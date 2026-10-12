//! Satu pintu obrolan: `chat`, reaksi emosi, dan umpan balik.
use super::*;

impl Engine {
    /// Pintu masuk bilah obrolan.
    pub(super) async fn chat(&mut self, text: String) {
        let t = text.trim().to_string();
        if t.is_empty() {
            return;
        }
        if let Some(rest) = t.strip_prefix('/') {
            if !rest.to_lowercase().starts_with("key") && !rest.to_lowercase().starts_with("gemini key") && !rest.to_lowercase().starts_with("email imap") {
                self.tx.send(Event::Chat(ChatLine { who: Who::Boss, text: t.clone(), media: Vec::new() }));
            }
            return self.slash(rest).await;
        }
        // Kru (pola CrewAI): "kru <tujuan>" memecah tugas ke peneliti/pelaksana/penulis.
        if let Some(g) = t.strip_prefix("kru ").or_else(|| t.strip_prefix("Kru ")).or_else(|| t.to_lowercase().strip_prefix("kerahkan kru untuk ").map(|_| &t[19..])) {
            let g = g.trim().to_string();
            self.tx.send(Event::Chat(ChatLine { who: Who::Boss, text: t.clone(), media: Vec::new() }));
            return self.kru(&g).await;
        }
        // Jalan pintas: ketik "obrolan"/"chat"/"buka obrolan" langsung membuka layar obrolan besar.
        if ["obrolan", "chat", "buka obrolan", "buka chat", "obrolan besar", "layar obrolan", "ngobrol"].contains(&t.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase().as_str()) {
            crate::api::OPEN_BIG.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return;
        }
        self.tx.send(Event::Chat(ChatLine { who: Who::Boss, text: t.clone(), media: Vec::new() }));
        crate::bandit::engaged(); // Bos merespons: hadiah untuk tindakan proaktif barusan
        crate::graf::ingest("chat", &t);
        crate::adegan::note_chat();
        let low = t.to_lowercase();
        if low == "mulai sekarang" {
            crate::bec::force_now();
            return self.agent_say(3, "Siap, tes BEC dimulai sekarang 📘");
        }
        if low == "nanti sesuai jadwal" {
            return self.agent_say(3, "Oke, nanti sesuai jadwal ya 👍");
        }
        if ["berisik", "jangan ganggu", "diam dulu", "diam dong", "stop ganggu"].iter().any(|k| low.contains(k)) {
            crate::bandit::feedback(false);
            crate::bandit::set_quiet(60);
            return self.agent_say(0, "Siap, aku diam dulu sejam. Panggil aku kapan saja lewat obrolan ini.");
        }
        if crate::emosi::krisis(&t) {
            self.tx.send(Event::Mood { emo: "sedih".into(), level: 1.0 });
            return self.agent_say(0, crate::emosi::PESAN_KRISIS);
        }
        // Dialog state: lengkapi perintah yang tadi kekurangan slot (jam/path), kecuali ini perintah baru yang utuh.
        let t = match crate::ml::dst_take_full() {
            Some((prev, force)) if force || t.split_whitespace().count() <= 3 || alat::plan_offline(&t).is_none() => format!("{prev} {t}"),
            _ => t,
        };
        let emo_hit = self.react_emotion(&t);
        if emo_hit && alat::plan_offline(&t).is_none() {
            return;
        }
        if low == "mulai tur" {
            return self.agent_say(0, GUIDE);
        }
        crate::hewan::touch();
        crate::persona::observe(&t);
        crate::makan::catat("obrolan");
        if let Some(m) = crate::buddy::add_xp("obrolan") {
            self.tx.notice(m.clone());
            self.tx.send(Event::Mood { emo: "gembira".into(), level: 1.0 });
            self.tx.sys(m);
        }
        let tt = t.clone();
        let saved = tokio::task::spawn_blocking(move || crate::fakta::extract(&tt)).await.unwrap_or_default();
        if !saved.is_empty() {
            self.tx.sys(format!("🧠 Kuingat: {}", saved.join("; ")));
        }
        if ["apa yang kamu ingat", "apa yang kau ingat", "apa yang kamu tahu tentang saya", "kamu ingat apa tentang saya"].iter().any(|k| low.contains(k)) {
            return self.agent_say(3, &crate::fakta::daftar());
        }
        if low.split_whitespace().count() <= 5 && ["ganti", "jadi", "pakai", "beralih", "pindah"].iter().any(|k| low.contains(k)) {
            let names = ["nova", "volt", "sentry", "memo"];
            if let Some(i) = names.iter().position(|n| low.contains(n)) {
                crate::konteks::OVERRIDE.store(i, std::sync::atomic::Ordering::Relaxed);
                return self.agent_say(i, &format!("Oke, Bos — sekarang {} yang tampil dan menemanimu. Ketik 'agen otomatis' untuk kembali ke pemilihan otomatis.", PERSONAS[i].name));
            }
        }
        if low == "agen otomatis" {
            crate::konteks::OVERRIDE.store(4, std::sync::atomic::Ordering::Relaxed);
            return self.agent_say(0, "Siap, agen kembali dipilih otomatis menurut konteks.");
        }
        if low == "jelaskan error" {
            let e = crate::bangun::last_error();
            if let Some(m) = crate::buddy::add_xp("perbaikan") {
                self.tx.sys(m);
            }
            if e.is_empty() {
                return self.agent_say(1, "Tidak ada error build yang tersimpan.");
            }
            if crate::gemini::ready() {
                self.tx.send(Event::Thinking(Some(1)));
                let r = crate::gemini::generate(&self.http, "Kamu VOLT, ahli pemrograman yang ramah. Jelaskan error build berikut dalam Bahasa Indonesia: penyebab paling mungkin dan langkah perbaikan singkat (maksimal 6 kalimat).", &e, false, false).await;
                self.tx.send(Event::Thinking(None));
                return match r {
                    Ok(t) => self.agent_say(1, &t),
                    Err(err) => self.agent_say(1, &format!("Gemini tidak tersedia ({err}). Error-nya:\n{e}")),
                };
            }
            return self.agent_say(1, &format!("Error utama:\n{e}\nTanyakan dengan: tanya <pesan error>"));
        }
        if let Some(r) = crate::hewan::resolve_paksa(&t) {
            return self.agent_say(0, &r);
        }
        if let Some(r) = crate::hewan::resolve(&t, &self.tx) {
            return self.agent_say(0, &r);
        }
        if let Some(cmd) = crate::seret::resolve(&t) {
            return self.start_mission(cmd).await;
        }
        // Penilaian tebakan ML terakhir: "Salah"/"bukan itu" -> catat sebagai negatif lalu coba lagi (lewat Gemini bila ada).
        if let Some((q, l)) = self.last_ml.clone() {
            let w = low.trim_matches(|c: char| !c.is_alphanumeric() && c != ' ');
            if ["salah", "bukan", "bukan itu", "salah paham", "tidak sesuai"].contains(&w) {
                self.last_ml = None;
                crate::ml::penalize(&q, &l);
                self.tx.sys("🧠 Dicatat: tebakan tadi salah — aku tidak akan mengulanginya untuk kalimat itu. Kucoba pahami lagi…");
                return self.start_mission(q).await;
            }
            if ["ya", "betul", "benar", "sesuai"].contains(&w) {
                self.last_ml = None;
                crate::ml::learn(&l, &q);
                return self.tx.sys("🧠 Terima kasih — kujadikan contoh belajar.");
            }
        }
        for k in ["diskusikan", "diskusi", "debat", "bahas bareng", "bahas bersama", "menurut kalian", "rapat tim"] {
            if low.starts_with(k) || low.contains(&format!(" {k} ")) || low.ends_with(&format!(" {k}")) {
                let topic = t[t.to_lowercase().find(k).map(|i| i + k.len()).unwrap_or(0)..].trim().trim_start_matches(|c: char| c == ':' || c == ',' || c == ' ').to_string();
                return self.diskusi(if topic.is_empty() { t.clone() } else { topic }).await;
            }
        }
        if low.split_whitespace().count() <= 8 && ["sholat", "salat", "shalat", "adzan", "azan"].iter().any(|k| low.contains(k)) {
            let kota = low.split(" di ").nth(1).map(|k| format!("kota {}", k.trim())).unwrap_or_default();
            return self.slash(&format!("sholat {kota}")).await;
        }
        if low.split_whitespace().count() <= 5 && low.contains("pilih") && (low.contains("file") || low.contains("berkas") || low.contains("folder")) {
            return self.slash(if low.contains("folder") { "pilih folder" } else { "pilih" }).await;
        }
        // Jawaban untuk pertanyaan "tugas utama sudah selesai?"
        if crate::tugas::awaiting_done() {
            if ["sudah", "udah", "selesai", "ya", "iya"].contains(&low.as_str()) {
                return self.agent_say(3, &crate::tugas::mark_done());
            }
            if ["belum", "tidak", "nanti"].contains(&low.as_str()) {
                crate::tugas::clear_await();
                crate::saran::offer(&self.tx, "Belum selesai ya. Mau kuingatkan lagi 1 jam lagi untuk melanjutkannya?", "ingatkan 60 menit lagi lanjutkan tugas utama", "tugas");
                return;
            }
        }
        // Saran tindakan yang dijawab Ok -> jalankan perintahnya
        if !crate::berkas::has_pending() && !crate::klip::has_pending() {
            if let Some(cmd) = crate::saran::resolve(&t) {
                self.agent_say(0, "Siap, kukerjakan 👍");
                return self.start_mission(cmd).await;
            }
        }
        if crate::berkas::has_pending() {
            if crate::klip::is_yes(&t) {
                if let Some(msg) = crate::berkas::confirm() {
                    return self.agent_say(0, &msg);
                }
            } else if crate::klip::is_no(&t) {
                crate::berkas::clear();
                return self.agent_say(0, "Baik, tidak jadi. Tidak ada yang kuubah 👍");
            }
        }
        if crate::klip::has_pending() {
            if crate::klip::is_yes(&t) {
                if let Some(lines) = crate::klip::help() {
                    return crate::adegan::play(&self.tx, lines);
                }
            } else if crate::klip::is_no(&t) {
                crate::klip::clear();
                return self.agent_say(0, "Baik, tidak jadi.");
            }
        }
        self.start_mission(crate::rawat::resolve(&t)).await;
    }

    /// Balasan empatik berbasis emosi; gaya dipilih bandit. True bila emosi signifikan terdeteksi.
    pub(super) fn react_emotion(&mut self, t: &str) -> bool {
        use crate::emosi::{analisis, balasan, gaya_diizinkan, Emosi};
        let (emo, lvl) = analisis(t);
        // Nilai balasan emosi sebelumnya: apakah suasana hati membaik?
        if let Some((ctx, gaya, at, prev)) = self.last_emo.take() {
            if at.elapsed().as_secs() < 900 {
                let r = if emo.valensi() > prev + 0.05 { 1.0 } else if emo.valensi() >= prev - 0.05 { 0.5 } else { 0.0 };
                crate::bandit::reward(&ctx, &gaya, r);
            }
        }
        if emo == Emosi::Netral || lvl < 0.35 {
            return false;
        }
        self.tx.send(Event::Mood { emo: emo.key().into(), level: lvl });
        let ctx = crate::bandit::ctx(&format!("emosi:{}", emo.key()));
        let gaya = crate::bandit::choose(&ctx, gaya_diizinkan(emo));
        if gaya != "diam" {
            if let Some(msg) = balasan(emo, &gaya, self.turns + t.len()) {
                let mut lines = vec![(if gaya == "humor" { 1 } else { 0 }, msg)];
                if gaya == "humor" {
                    lines.push((1, crate::meme::meme(t, self.turns + t.len())));
                }
                crate::adegan::play(&self.tx, lines);
            }
        }
        self.last_emo = Some((ctx, gaya, Instant::now(), emo.valensi()));
        true
    }

    pub(super) fn feedback(&mut self, good: bool) {
        if crate::bandit::feedback(good) {
            self.tx.send(Event::FeedbackAsk(false));
            self.agent_say(0, if good { "Terima kasih, kucatat 👍 — aku akan lebih sering begini." } else { "Baik, kucatat 👎. Aku diam 30 menit dan belajar dari ini." });
        } else {
            self.agent_say(0, "Belum ada tindakan proaktifku yang bisa dinilai sekarang.");
        }
    }

}
