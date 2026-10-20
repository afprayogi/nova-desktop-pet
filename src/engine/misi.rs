//! Misi komputer: perencanaan alat (aturan, semantik, Gemini, Llama), eksekusi langkah, dan persetujuan.
use super::*;

impl Engine {
    /// Verifikasi tebakan lokal dengan rencana model. True bila beda dan Bos ditanya (misi berhenti menunggu jawaban).
    async fn bandingkan(&mut self, goal: &str, a: &Option<Vec<Call>>, b: &[Call], sumber: &str) -> bool {
        let a1 = a.as_deref().and_then(|v| if let [x] = v { Some(x) } else { None });
        let crate::putusan::Nilai::Beda(x, y) = crate::putusan::nilai(goal, a1, b, sumber) else { return false };
        let o = crate::putusan::tunggu(goal, &x, &y);
        self.agent_say(0, &format!("Aku ragu membaca \"{goal}\" — {} vs {}. Pilih yang benar supaya kuingat dan tidak salah lagi.", o[0], o[1]));
        self.tx.send(Event::Ask { question: "Maksudmu yang mana?".into(), options: o });
        self.running = false;
        self.tx.send(Event::Running(false));
        true
    }

    pub(super) async fn start_mission(&mut self, goal: String) {
        let w = goal.trim().trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
        if ["ya", "iya", "ok", "oke", "siap", "sip", "nanti", "tidak", "enggak", "gak", "y", "n"].contains(&w.as_str()) {
            return self.agent_say(0, "Oke, Bos 👍 Ada yang mau kubantu?");
        }
        // Putusan tersimpan (kasus.json / terverifikasi / dikonfirmasi) didahulukan; koreksi Bos memaksa kalimat ditanyakan ke model.
        // "jawab pakai gemini aja ...": Bos meminta model, bukan membuka situs Gemini. Frasa dibuang dari kalimat, lalu aturan lokal dilewati.
        let (goal, minta_model) = {
            let low = goal.to_ascii_lowercase();
            match ["jawab pakai gemini", "jawab pake gemini", "jawab dengan gemini", "pakai gemini", "pake gemini", "gunakan gemini", "pakai model", "jawab pakai ai"].iter().find_map(|k| low.find(k).map(|i| (i, k.len()))) {
                Some((i, n)) if !low.contains("buka") => {
                    let mut g = format!("{} {}", &goal[..i], &goal[i + n..]);
                    for w in [" aja ", " saja ", " dong "] {
                        g = g.replacen(w, " ", 1);
                    }
                    (g.split_whitespace().collect::<Vec<_>>().join(" "), true)
                }
                _ => (goal, false),
            }
        };
        let tolak = crate::putusan::ambil_paksa(&goal);
        let paksa = tolak.is_some() || minta_model;
        if goal.split_whitespace().count() >= 3 && !goal.starts_with('/') {
            let m = crate::ingat::dipakai(&goal);
            if !m.is_empty() {
                self.tx.sys(m);
            }
        }
        let tersimpan = if paksa { None } else { crate::putusan::cari(&goal) };
        let mut offline = if paksa { None } else { tersimpan.clone().map(|c| vec![c]).or_else(|| crate::keahlian::cocok(&goal)).or_else(|| alat::plan_offline(&goal)) };
        let mut g_failed = false;
        let only_tanya = offline.as_ref().is_some_and(|p| p.len() == 1 && p[0].tool == "tanya");
        let mut from_ml = tersimpan.is_none() && !paksa && alat::FROM_ML.load(std::sync::atomic::Ordering::Relaxed);
        // Koreksi Bos: coba kandidat lokal berikutnya (makna) dulu; baru ke model bila habis. Hasilnya tetap diverifikasi model.
        if let Some(t) = &tolak {
            if let Ok(sk) = crate::semantik::scores(&self.http, &goal).await {
                let ditolak = alat::label_of(t);
                if let Some(c) = sk.iter().find(|(l, s)| *s >= 0.6 && Some(l.as_str()) != ditolak && !["tanya", "ngobrol"].contains(&l.as_str())).and_then(|(l, _)| alat::call_for(l, &goal)) {
                    offline = Some(vec![c]);
                    from_ml = true;
                }
            }
        }
        // Pemahaman semantik (embedding): niat aksi yang jelas dipakai langsung, tanpa panggilan perencana yang lebih mahal.
        if !paksa && (offline.is_none() || only_tanya || from_ml) {
            if let Some((label, skor)) = crate::semantik::classify(&self.http, &goal).await {
                if !["tanya", "ngobrol"].contains(&label.as_str()) {
                    if let Some(c) = alat::call_for(&label, &goal) {
                        offline = Some(vec![c]);
                        from_ml = skor < 0.85; // keyakinan sedang: diverifikasi model
                        crate::ml::learn(&label, &goal);
                    }
                }
            }
        }
        // Layanan web berpemicu umum (layanan.json): hanya bila tidak ada kemampuan lokal yang cocok (atau hanya aturan "cari/buka" yang lemah).
        if offline.as_ref().is_none_or(|p| p.len() == 1 && (p[0].tool == "cari" || (p[0].tool == "open" && !p[0].arg.starts_with("http") && crate::aplikasi::cari(&p[0].arg).is_none()))) {
            if let Some(c) = crate::aturan::layanan_cadangan(&goal) {
                offline = Some(vec![c]);
                from_ml = false;
            }
        }
        if offline.is_none() && !crate::gemini::available() {
            use crate::niat::Intent;
            match crate::niat::classify(&self.http, &goal).await {
                Ok(Intent::SetReminder { text, when }) => {
                    offline = Some(vec![alat::Call { tool: "remind".into(), arg: format!("{when} {text}") }]);
                    from_ml = false;
                }
                Ok(Intent::HealthQuery { .. }) => {
                    offline = alat::call_for("kesehatan", &goal).map(|c| vec![c]);
                    from_ml = false;
                }
                _ => {}
            }
        }
        let only_tanya = offline.as_ref().is_some_and(|p| p.len() == 1 && p[0].tool == "tanya");
        if (offline.is_none() || only_tanya || from_ml) && crate::gemini::available() {
            self.tx.send(Event::Thinking(Some(0)));
            self.act("gemini", "", 9000);
            let r = crate::gemini::plan(&self.http, &goal).await;
            self.tx.send(Event::Thinking(None));
            match r {
                Ok(p) if !p.calls.is_empty() => {
                    // Gemini = kunci jawaban: kalimat ini menjadi contoh latihan ML bila hanya satu alat.
                    if p.calls.len() == 1 {
                        if let Some(l) = alat::label_of(&p.calls[0].tool) {
                            crate::ml::learn(l, &goal);
                        }
                    }
                    if self.bandingkan(&goal, &offline, &p.calls, "gemini").await {
                        return;
                    }
                    if !p.reply.trim().is_empty() {
                        self.agent_say(0, &research::truncate(&p.reply, 300));
                    }
                    offline = Some(p.calls)
                }
                Ok(p) if !p.reply.trim().is_empty() => {
                    crate::ingat::episode("misi", &goal);
                    let _ = self.mem.add_message("BOS", &goal);
                    let txt = research::truncate(&p.reply, 1500);
                    crate::teknik::simpan_jawaban("gemini", &goal, &p.reply);
                    self.tawarkan_keahlian(&goal);
                    return self.agent_say(0, &if p.cached { format!("💾 {txt}") } else { txt });
                }
                Ok(_) => {}
                Err(e) => {
                    g_failed = true;
                    let _ = e; // pesan peralihan ditampilkan oleh take_event()
                }
            }
        }
        if let Some(ev) = crate::gemini::take_event() {
            self.tx.sys(ev);
        }
        crate::lokal::unload_if_requested(&self.http).await;
        let unreliable = offline.is_none() || only_tanya || from_ml;
        let local_mode = unreliable && (!crate::gemini::available() || g_failed) && !offline.as_ref().is_some_and(|p| p.len() == 1 && p[0].tool != "tanya" && !from_ml);
        let mut planned = false;
        if local_mode && !crate::lokal::looks_like_code(&goal) {
            // Llama merencanakan alat (keluaran terkunci ke skema JSON) sehingga bisa memakai semua perintah/skill NOVA
            self.tx.send(Event::Thinking(Some(0)));
            self.act("pikir", "", 12000);
            let pr = crate::lokal::plan(&self.http, &goal).await;
            self.tx.send(Event::Thinking(None));
            if let Ok((calls, reply)) = pr {
                if !calls.is_empty() {
                    if self.bandingkan(&goal, &offline, &calls, "llama").await {
                        return;
                    }
                    if !reply.trim().is_empty() {
                        self.agent_say(0, &research::truncate(&reply, 200));
                    }
                    offline = Some(calls);
                    planned = true;
                    from_ml = false;
                }
            }
        }
        if local_mode && !planned {
            // Gemini mati/gagal: langsung ke model lokal (Llama; pertanyaan koding ke model koding), dengan konteks percakapan & memori
            let code = crate::lokal::looks_like_code(&goal);
            self.tx.send(Event::Thinking(Some(if code { 1 } else { 0 })));
            self.act(if code { "ketik" } else { "pikir" }, "", 12000);
            // pertanyaan teknik: basis pengetahuan lokal menjadi bahan (RAG) agar Llama kecil tidak mengarang
            let prompt = if only_tanya {
                format!("{goal}

[Catatan basis pengetahuan; pakai hanya bila relevan, abaikan bila tidak nyambung]
{}", research::truncate(&crate::teknik::tanya(&goal), 900))
            } else {
                goal.clone()
            };
            let r = if code { crate::lokal::code_chat(&self.http, &prompt).await } else { crate::lokal::chat(&self.http, &prompt).await };
            self.tx.send(Event::Thinking(None));
            if let Ok(t) = r {
                let _ = self.mem.add_message("BOS", &goal);
                crate::ingat::episode("misi", &goal);
                return self.agent_say(if code { 1 } else { 0 }, &t);
            }
        }
        let only_tanya = offline.as_ref().is_some_and(|p| p.len() == 1 && p[0].tool == "tanya");
        if offline.is_none() || (only_tanya && self.llm_on) {
            if self.llm_on && !self.lock().pool().is_empty() {
                crate::ingat::episode("misi", &goal);
                return self.llm_reply(goal).await;
            }
            self.tawarkan_keahlian(&goal);
            // Tanpa model, pertanyaan bebas dijawab lewat pencarian hibrida (RAG) di memori lokal.
            offline = Some(vec![Call { tool: "tanya".into(), arg: goal.clone() }]);
        }
        // Rencana yang hanya tebakan ML (tanpa Gemini): minta Bos menilai agar ML belajar dari koreksi.
        self.last_ml = None;
        if from_ml && offline.as_ref().is_some_and(|p| p.len() == 1) {
            if let Some(l) = offline.as_ref().and_then(|p| alat::label_of(&p[0].tool)) {
                self.last_ml = Some((goal.clone(), l.to_string()));
                self.tx.send(Event::Ask { question: "Sesuai maksudmu?".into(), options: vec!["Ya".into(), "Salah".into()] });
            }
        }
        if let Some([c]) = offline.as_deref() {
            crate::putusan::set_last(&goal, c);
        }
        self.running = false;
        self.tx.send(Event::Running(false));
        crate::ingat::episode("misi", &goal);
        let _ = self.mem.add_message("BOS", &goal);
        if let Some(q) = offline.as_ref().and_then(|qs| qs.iter().find_map(|c| crate::ml::missing_slot(&c.tool, &c.arg))) {
            crate::ml::dst_set(&goal);
            return self.agent_say(0, q);
        }
        if let Some(queue) = offline {
            let plan = format!(
                "Mode tanpa model, {} langkah: {}",
                queue.len(),
                queue.iter().map(|c| c.tool.as_str()).collect::<Vec<_>>().join(" → ")
            );
            if queue.len() > 1 {
                self.agent_say(0, &plan);
            }
            let (judul, total) = (research::truncate(&goal, 40), queue.len());
            if total > 1 {
                self.job = Some((crate::progres::mulai(&judul, "menyiapkan langkah"), total));
            }
            self.mission = Some(Mission { goal, plan, obs: Vec::new(), step: 0, bad: 0, pending: None, offline: true, queue });
            return;
        }
        self.assign_seats(false);
        let sys = format!(
            "{}\nBuat rencana MAKSIMAL 4 langkah pendek untuk tujuan Bos, memakai alat berikut:\n{}",
            Self::persona_sys(0, &goal),
            crate::registry::bantuan()
        );
        let plan = self
            .ask(0, sys, goal.clone(), 300)
            .await
            .unwrap_or_else(|| "Langsung kerjakan.".into());
        self.agent_say(0, &plan);
        self.job = Some((crate::progres::mulai(&research::truncate(&goal, 40), "merencanakan"), 0));
        self.mission = Some(Mission { goal, plan, obs: Vec::new(), step: 0, bad: 0, pending: None, offline: false, queue: Vec::new() });
    }

    /// Permintaan aksi yang tak dikenali: tawarkan membuat keahlian baru (sekali per kalimat, tanpa memaksa).
    fn tawarkan_keahlian(&self, goal: &str) {
        if crate::keahlian::tampak_perintah(goal) && crate::assistant::once_key(&format!("kh-{}", research::truncate(&goal.to_lowercase(), 60))) {
            crate::keahlian::set_calon(goal);
            self.tx.send(Event::Ask { question: "Aku belum punya keahlian untuk itu. Mau kupelajari lewat internet dan kubuat keahlian barunya?".into(), options: vec!["buat keahlian ini".into(), "batal keahlian".into()] });
        }
    }

    pub(super) async fn finish_mission(&mut self, note: &str) {
        let Some(m) = self.mission.take() else { return };
        if let Some((j, _)) = self.job.take() {
            crate::progres::selesai(j, note);
        }
        self.tx.send(Event::Approval(None));
        if m.offline && m.obs.len() <= 1 {
            return; // satu langkah: jawabannya sudah tampil
        }
        if m.offline {
            let rep = format!("Selesai. Ringkasan hasil:\n{}", research::truncate(&m.obs.join("\n"), 1500));
            return self.agent_say(0, &rep);
        }
        let user = format!(
            "TUJUAN: {}\nCATATAN: {note}\nHASIL:\n{}\nTulis laporan akhir untuk Bos, maksimal 5 kalimat.",
            m.goal,
            research::truncate(&m.obs.join("\n"), 5000)
        );
        let rep = self
            .ask(0, Self::persona_sys(0, &m.goal), user, 350)
            .await
            .unwrap_or_else(|| note.to_string());
        self.agent_say(0, &rep);
    }

    pub(super) async fn mission_step(&mut self) {
        let Some(m) = self.mission.as_mut() else { return };
        if m.offline {
            if m.queue.is_empty() {
                return self.finish_mission("Semua langkah selesai.").await;
            }
            let call = m.queue.remove(0);
            if let Some((j, t)) = self.job {
                crate::progres::langkah(j, t.saturating_sub(m.queue.len()).max(1), t, &call.tool);
            }
            return self.exec_call(call).await;
        }
        if m.step >= MAX_STEPS {
            return self.finish_mission("Batas langkah tercapai.").await;
        }
        let joined = m.obs.join("\n");
        let skip = joined.chars().count().saturating_sub(5000);
        let obs: String = joined.chars().skip(skip).collect();
        let sys = format!(
            "{}\nPilih SATU langkah berikutnya. Balas HANYA JSON satu baris: \
{{\"tool\":\"nama\",\"arg\":\"...\",\"say\":\"alasan singkat\"}} atau {{\"done\":true,\"say\":\"hasil\"}} bila tujuan tercapai. \
Jangan mengulang langkah yang sudah berhasil. Alat:\n{}",
            Self::persona_sys(0, &m.goal),
            crate::registry::bantuan()
        );
        let user = format!(
            "TUJUAN: {}\nRENCANA: {}\nHASIL SEBELUMNYA:\n{}",
            m.goal,
            m.plan,
            if obs.is_empty() { "(belum ada)" } else { &obs }
        );
        let Some(t) = self.ask(0, sys, user, 300).await else {
            return self.finish_mission("Model tidak merespons.").await;
        };
        let Some(m) = self.mission.as_mut() else { return };
        m.step += 1;
        if let Some((j, _)) = self.job {
            crate::progres::set(j, (m.step * 100 / MAX_STEPS).min(95) as u8, &format!("langkah {} · berpikir", m.step));
        }
        let Some(v) = alat::parse_json(&t) else {
            m.bad += 1;
            m.obs.push("(balasan bukan JSON; balas persis format JSON)".into());
            if m.bad >= 3 {
                return self.finish_mission("Operator gagal memberi perintah valid.").await;
            }
            return;
        };
        let say = v["say"].as_str().unwrap_or("").to_string();
        if v["done"].as_bool() == Some(true) {
            if !say.is_empty() {
                self.agent_say(0, &say);
            }
            return self.finish_mission("Operator menyatakan selesai.").await;
        }
        let call = Call {
            tool: v["tool"].as_str().unwrap_or("").trim().to_lowercase(),
            arg: v["arg"].as_str().unwrap_or("").to_string(),
        };
        self.agent_say(0, &format!("{say}\n▶ {} {}", call.tool, short(&call.arg)));
        self.exec_call(call).await;
    }

    pub(super) async fn exec_call(&mut self, call: Call) {
        let label = format!("{} {}", call.tool, short(&call.arg));
        match alat::assess(&call) {
            Risk::Block(r) => {
                self.agent_say(2, &format!("⛔ DITOLAK: {r}"));
                self.mission.as_mut().unwrap().obs.push(format!("{label} => DIBLOKIR: {r}"));
            }
            Risk::Safe => {
                self.act(act_kind(&call), "", 2800);
                let r = alat::run(&call, &self.http).await;
                self.emit_term(&call, &r);
                if call.tool == "tanya" && crate::teknik::ada_nilai() {
                    self.tx.send(Event::Ask { question: "Jawaban dari model tadi benar?".into(), options: vec!["Jawaban benar ✔".into(), "Jawaban salah ✘".into()] });
                }
                // Video tidak ada di laptop → tawarkan mencarinya di YouTube dengan kata kunci yang sama.
                if call.tool == "cari" && r.starts_with("Tidak ketemu") && r.contains("mp4/mkv") {
                    let q = r.split("Kata yang kucari: ").nth(1).and_then(|x| x.split(" · ").next()).unwrap_or("").replace(',', "");
                    if !q.trim().is_empty() {
                        crate::saran::offer_always(&self.tx, "Video itu tidak ada di laptop. Cari di YouTube?", &format!("putar video youtube {}", q.trim()), "video");
                    }
                }
                let speaker = match call.tool.as_str() {
                    "tanya" | "ringkas" | "kurikulum" | "meme" => Some(1),
                    "cari" | "bukahasil" => Some(3),
                    "rapikan" | "urungkan" => Some(0),
                    "apps" => Some(2),
                    "berita" => Some(3),
                    "kepribadian" | "keahlian" | "resume" | "selera" | "pembaruan" | "kerja" | "jadwalkan" | "pemicu" | "skrip" | "pesan" => Some(0),
                    "kalk" | "saranproyek" | "cerdas" | "privasi" | "aturanberkas" | "githubissue" => Some(1),
                    "mcp" | "plugin" => Some(0),
                    "tugas" => Some(3),
                    "web" | "wifi" | "port" | "bukafile" | "bukapath" | "winapp" | "waktu" | "cli" => Some(0),
                    "jadwal" => Some(3),
                    "otomatis" => Some(0),
                    "musik" => Some(0),
                    "ngobrol" => Some(0),
                    "baterai" | "health" => Some(2),
                    "agenda" | "habit" | "tidur" | "remind" => Some(3),
                    _ => None,
                };
                match speaker {
                    Some(i) => self.agent_say(i, &research::truncate(&r, 1400)),
                    None => self.tx.sys(format!("⚙ {} → {}", call.tool, research::truncate(&r, if r.contains("CHART ") { 3000 } else { 500 }))),
                }
                if call.tool == "jadwalkan" && crate::penjadwal::ada_draft() {
                    self.tx.send(Event::Ask { question: "Buat jadwal ini di Windows Task Scheduler?".into(), options: vec!["buat jadwal ini".into(), "batal jadwal ini".into()] });
                }
                if call.tool == "keahlian" && crate::keahlian::ada_impor() {
                    self.tx.send(Event::Ask { question: "Impor keahlian ini?".into(), options: vec!["impor keahlian ini".into(), "batal impor keahlian".into()] });
                }
                if call.tool == "skrip" && crate::skrip::ada_draft() {
                    self.tx.send(Event::Ask { question: "Simpan skrip ini (kode sudah kamu lihat)?".into(), options: vec!["simpan skrip".into(), "batal skrip".into()] });
                }
                if call.tool == "pemicu" && crate::pemicu::ada_draft() {
                    self.tx.send(Event::Ask { question: "Aktifkan pemicu ini?".into(), options: vec!["buat pemicu ini".into(), "batal pemicu ini".into()] });
                }
                if call.tool == "keahlian" && crate::keahlian::ada_draft() {
                    self.tx.send(Event::Ask { question: "Simpan keahlian ini?".into(), options: vec!["simpan keahlian".into(), "batal keahlian".into()] });
                }
                if call.tool == "tanya" {
                    crate::adegan::on_answer(&self.tx, &call.arg, &r);
                }
                if crate::berkas::has_pending() {
                    self.tx.send(Event::Ask { question: "Lanjutkan?".into(), options: vec!["ya".into(), "tidak".into()] });
                }
                if call.tool == "cari" && crate::indeks::last_count() > 1 {
                    self.tx.send(Event::Ask { question: "Buka folder hasil lain?".into(), options: (2..=crate::indeks::last_count().min(4)).map(|i| format!("buka folder {i}")).collect() });
                }
                self.mission.as_mut().unwrap().obs.push(format!("{label} => {r}"));
            }
            Risk::Confirm(reason) => {
                self.agent_say(2, &format!("⚠ Butuh persetujuan Bos: {reason}"));
                crate::notify::toast("NOVA butuh izin", &format!("{label}\n{reason}"));
                self.tx.send(Event::Approval(Some(Approval {
                    tool: call.tool.clone(),
                    arg: call.arg.clone(),
                    reason,
                })));
                self.mission.as_mut().unwrap().pending = Some(call);
            }
        }
    }

}
