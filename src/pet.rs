//! Desktop pet / screenmate NOVA: satu-satunya tampilan aplikasi. Satu peliharaan yang berganti rupa (NOVA/VOLT/SENTRY/MEMO)
//! menurut konteks, plus bilah obrolan sederhana (klik peliharaan untuk membuka/menutup). Keadaan dibaca dari API lokal
//! milik proses tray. FSM: Idle -> Wander -> Idle; Sleep bila Bos diam lama/jam tidur; Interrupted bila dipanggil/ada peringatan.
//! Klik = buka/tutup obrolan, klik kanan = tidur/bangun, seret = pindahkan. Animasi hemat CPU (3-12 fps).
use crate::agents::{self, Animal, Frame};
use crate::bec;
use crate::lukis::*;
use eframe::egui::{self, pos2, vec2, Align2, Color32, FontId, Key, RichText, Rect, Sense, Stroke, ViewportCommand};
use serde::Deserialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub(crate) const W: f32 = 330.0;
pub(crate) const H: f32 = 150.0;
/// Tinggi dock ketik permanen di bawah peliharaan.
const DOCK: f32 = 0.0;
const WH: f32 = H + DOCK;
pub(crate) const SCALE: f32 = 5.0;
const LINE_SECS: f32 = 8.0;
const SLEEP_AFTER_IDLE: u64 = 300; // 5 menit tanpa gerakan mouse/keyboard -> tidur

#[derive(Deserialize, Clone, Default)]
struct ChatLine {
    seq: u64,
    who: i32,
    text: String,
}

#[derive(Deserialize, Clone, Default)]
struct Appr {
    tool: String,
    arg: String,
    reason: String,
}

#[derive(Deserialize, Clone, Default)]
struct Prog {
    judul: String,
    label: String,
    pct: u8,
    selesai: bool,
    gagal: bool,
}

#[derive(Deserialize, Clone, Default)]
struct Poll {
    mood: String,
    status: String,
    next: String,
    scene_id: u64,
    scene_age: f32,
    lines: Vec<(usize, String)>,
    active: usize,
    mode: String,
    user_idle: u64,
    chat: Vec<ChatLine>,
    approval: Option<Appr>,
    #[serde(default)]
    progress: Vec<Prog>,
    thinking: bool,
    open_chat: u64,
    #[serde(default)]
    open_big: u64,
    emo: String,
    emo_age: f32,
    feedback: bool,
    macros: Vec<String>,
    bec_due: bool,
    ask: Option<Ask>,
    act: String,
    act_text: String,
    act_age: f32,
    act_ms: u32,
    #[serde(default)]
    notices: Vec<String>,
    #[serde(default)]
    notice_seq: u64,
    #[serde(default)]
    karakter: String,
    #[serde(default)]
    topi: String,
    #[serde(default)]
    term_seq: u64,
    #[serde(default)]
    term: String,
    #[serde(default = "neg1")]
    ui_age: f32,
    #[serde(default)]
    typing: bool,
    #[serde(default)]
    media: String,
}

#[derive(Deserialize, Clone, Default)]
struct Ask {
    question: String,
    options: Vec<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum St {
    Idle,
    Wander,
    Sleep,
    Interrupted,
    /// jatuh setelah dilepas dari seretan (karakter)
    Fall,
    /// memanjat tepi layar (karakter)
    Climb,
}

struct Pet {
    who: Animal,
    st: St,
    st_until: Instant,
    fx_until: Option<Instant>,
    last_swap: Instant,
    rx: Receiver<Option<Poll>>,
    cmd: Sender<serde_json::Value>,
    fast: Arc<AtomicBool>,
    last: Option<Poll>,
    polled_at: Instant,
    fails: u32,
    ever: bool,
    started: Instant,
    shown: (u64, usize),
    bubble: Option<(String, Instant)>,
    placed: bool,
    pos: (f32, f32),
    dir: f32,
    last_frame: Instant,
    paused: bool,
    alerting: bool,
    chat_open: bool,
    input: String,
    focus_next: bool,
    open_seen: u64,
    big_seen: Option<u64>,
    wink_until: Option<Instant>,
    bec: Option<BecRun>,
    bec_rx: Receiver<Option<bec::TestSet>>,
    bec_tx: Sender<Option<bec::TestSet>>,
    bec_fetching: bool,
    bec_block_until: Instant,
    port: u16,
    web_ui: Option<std::process::Child>,
    ask_seen: String,
    /// (seq obrolan saat kirim, waktu tutup otomatis)
    await_reply: Option<(u64, Option<Instant>)>,
    hist: Vec<String>,
    hist_i: Option<usize>,
    collapse_at: Option<(Instant, String)>,
    /// Ukuran jendela saat ini dan titik kanan-bawah peliharaan (tetap saat jendela membesar/mengecil).
    size: (f32, f32),
    anchor: (f32, f32),
    popup: Option<Popup>,
    popup_h: f32,
    close_popup_flag: bool,
    show_notices: bool,
    notice_seen: u64,
    /// seq pesan Bos terakhir yang sudah diproses untuk popup balasan (apa pun sumbernya: dock, tombol, API).
    seen_user_seq: Option<u64>,
    seen_term: u64,
    // --- karakter gaya Shimeji ---
    kar: Option<Kar>,
    kar_name: String,
    ground_y: f32,
    fall_vy: f32,
    dragging: bool,
    /// Area sprite yang menerima mouse; di luar itu jendela transparan meneruskan klik ke aplikasi di bawahnya.
    hit: Rect,
    pass: bool,
    special: Option<(usize, f64, f64)>,
    next_special: Instant,
    wheel_open: bool,
    wheel_until: Instant,
    dock_hidden: bool,
    dock_size: (f32, f32),
}

#[derive(Clone)]
struct Popup {
    mono: bool,
    agent: usize,
    text: String,
    until: Instant,
}

pub fn run(port: u16) -> anyhow::Result<()> {
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([W, WH])
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_resizable(false)
            .with_taskbar(false)
            .with_title(agents::NAME),
        ..Default::default()
    };
    eframe::run_native(agents::NAME, opts, Box::new(move |cc| {
        apply_style(&cc.egui_ctx);
        Ok(Box::new(Pet::new(port)))
    })).map_err(|e| anyhow::anyhow!("{e}"))
}

fn common_prefix(c: &[&str]) -> String {
    let first = c[0];
    let mut n = first.len();
    for s in &c[1..] {
        n = n.min(first.chars().zip(s.chars()).take_while(|(a, b)| a == b).count());
    }
    first[..n].to_string()
}

fn token() -> String {
    std::fs::read_to_string(crate::api::token_path()).unwrap_or_default().trim().to_string()
}

impl Pet {
    fn new(port: u16) -> Self {
        let (tx, rx) = channel();
        let fast = Arc::new(AtomicBool::new(false));
        let f2 = fast.clone();
        std::thread::spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() else { return };
            let http = reqwest::Client::builder().timeout(Duration::from_secs(3)).build().unwrap();
            let url = format!("http://127.0.0.1:{port}/api/pet");
            loop {
                let r = rt.block_on(async { http.get(&url).header("x-nova-token", token()).send().await?.error_for_status()?.json::<Poll>().await });
                if tx.send(r.ok()).is_err() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(if f2.load(Ordering::Relaxed) { 700 } else { 2000 }));
            }
        });
        let (ctx_tx, ctx_rx) = channel::<serde_json::Value>();
        std::thread::spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() else { return };
            let http = reqwest::Client::builder().timeout(Duration::from_secs(5)).build().unwrap();
            let url = format!("http://127.0.0.1:{port}/api/cmd");
            while let Ok(v) = ctx_rx.recv() {
                let _ = rt.block_on(async { http.post(&url).header("x-nova-token", token()).json(&v).send().await });
            }
        });
        let (bec_tx, bec_rx) = channel();
        let now = Instant::now();
        Self {
            who: Animal::Nova,
            st: St::Idle,
            st_until: now + Duration::from_secs(4),
            fx_until: None,
            last_swap: now,
            rx,
            cmd: ctx_tx,
            fast,
            last: None,
            polled_at: now,
            fails: 0,
            ever: false,
            started: now,
            shown: (0, usize::MAX),
            bubble: None,
            placed: false,
            pos: (600.0, 500.0),
            dir: -1.0,
            last_frame: now,
            paused: false,
            alerting: false,
            chat_open: false,
            await_reply: None,
            hist: Vec::new(),
            hist_i: None,
            collapse_at: None,
            size: (W, WH),
            anchor: (0.0, 0.0),
            popup: None,
            popup_h: 0.0,
            close_popup_flag: false,
            show_notices: false,
            notice_seen: 0,
            seen_user_seq: None,
            seen_term: u64::MAX,
            kar: None,
            kar_name: String::new(),
            ground_y: 0.0,
            fall_vy: 0.0,
            dragging: false,
            hit: Rect::NOTHING,
            pass: false,
            special: None,
            next_special: Instant::now() + Duration::from_secs(25),
            wheel_open: false,
            wheel_until: Instant::now(),
            dock_hidden: false,
            dock_size: (0.0, 0.0),
            input: String::new(),
            focus_next: false,
            open_seen: 0,
            big_seen: None,
            wink_until: None,
            bec: None,
            bec_rx,
            bec_tx,
            bec_fetching: false,
            bec_block_until: now,
            port,
            web_ui: None,
            ask_seen: String::new(),
        }
    }

    fn pos_before_bec(&mut self) {
        // posisi kecil disimpan agar kembali ke tempat semula setelah tes
        if self.chat_open {
            self.pos = (self.anchor.0 - W, self.anchor.1 - WH);
        }
    }

    /// Baris ketik (dipakai di dock permanen dan di jendela obrolan): Tab melengkapi, ↑/↓ riwayat, Enter kirim.
    fn input_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, srv: &Poll) {
        if self.chat_open {
            ui.style_mut().override_font_id = Some(FontId::proportional(16.0));
        }
        let (tab, up, down) = ctx.input_mut(|i| (i.consume_key(egui::Modifiers::NONE, Key::Tab), i.consume_key(egui::Modifiers::NONE, Key::ArrowUp), i.consume_key(egui::Modifiers::NONE, Key::ArrowDown)));
        let cands = crate::cli::complete(self.input.trim());
        if tab && !cands.is_empty() {
            self.input = if cands.len() == 1 { format!("{} ", cands[0]) } else { common_prefix(&cands) };
        }
        if up && !self.hist.is_empty() {
            let i = self.hist_i.map(|i| i.saturating_sub(1)).unwrap_or(self.hist.len() - 1);
            self.hist_i = Some(i);
            self.input = self.hist[i].clone();
        }
        if down {
            if let Some(i) = self.hist_i {
                if i + 1 < self.hist.len() {
                    self.hist_i = Some(i + 1);
                    self.input = self.hist[i + 1].clone();
                } else {
                    self.hist_i = None;
                    self.input.clear();
                }
            }
        }
        if !cands.is_empty() && cands.len() <= 8 {
            ui.label(RichText::new(format!("Tab ⇥  {}", cands.join("   "))).small().color(Color32::from_rgb(130, 180, 255)));
        }
        ui.horizontal(|ui| {
            if ui.add(egui::Button::new(RichText::new("+").strong().size(16.0).color(Color32::from_rgb(130, 180, 255))).rounding(14.0).min_size(vec2(26.0, 28.0))).on_hover_text("Pilih berkas (klik) · atau seret berkas/folder ke sini").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                self.send(serde_json::json!({"Chat": "/pilih"}));
            }
            let te = ui.add_sized([ui.available_width() - 100.0, 28.0], egui::TextEdit::singleline(&mut self.input).hint_text("Ketik pesan untuk NOVA…"));
            if self.focus_next {
                te.request_focus();
                self.focus_next = false;
            }
            let enter = te.lost_focus() && ctx.input(|i| i.key_pressed(Key::Enter));
            if (pill(ui, "Kirim ▶", Color32::from_rgb(120, 190, 255), true).clicked() || enter) && !self.input.trim().is_empty() {
                let text = std::mem::take(&mut self.input);
                if self.hist.last() != Some(&text) {
                    self.hist.push(text.clone());
                    if self.hist.len() > 50 {
                        self.hist.remove(0);
                    }
                }
                self.hist_i = None;
                self.await_reply = Some((srv.chat.last().map(|l| l.seq).unwrap_or(0), None));
                self.send(serde_json::json!({"Chat": text}));
                self.focus_next = true;
            }
        });
    }

    fn say(&mut self, text: &str, secs: u64) {
        let t: String = clean_ui(text).chars().take(400).collect();
        self.bubble = Some((t, Instant::now() + Duration::from_secs(secs)));
    }

    fn goto(&mut self, st: St, secs: f32) {
        self.st = st;
        self.st_until = Instant::now() + Duration::from_secs_f32(secs);
    }

    fn scene_age(&self) -> Option<f32> {
        let p = self.last.as_ref()?;
        if p.scene_age < 0.0 {
            return None;
        }
        let age = p.scene_age + self.polled_at.elapsed().as_secs_f32();
        (age < p.lines.len() as f32 * LINE_SECS + 2.0).then_some(age)
    }

    /// Buka/tutup obrolan: ubah ukuran jendela dengan sudut kanan-bawah tetap di tempat.
    /// Buka antarmuka web (jendela aplikasi Edge/Chrome tanpa bilah alamat). True bila berhasil/sudah terbuka.
    fn open_web_ui(&mut self) -> bool {
        if self.last.as_ref().is_some_and(|p| p.ui_age >= 0.0 && p.ui_age < 6.0) {
            return true; // sudah terbuka: jangan membuat jendela kedua
        }
        if let Some(c) = self.web_ui.as_mut() {
            if c.try_wait().ok().flatten().is_none() {
                return true;
            }
            self.web_ui = None;
        }
        let url = format!("http://127.0.0.1:{}/ui", self.port);
        for exe in [r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe", r"C:\Program Files\Microsoft\Edge\Application\msedge.exe", r"C:\Program Files\Google\Chrome\Application\chrome.exe", r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe"] {
            if std::path::Path::new(exe).exists() {
                let profile = crate::config::data_dir().join("ui-profile");
                if let Ok(c) = std::process::Command::new(exe).arg(format!("--app={url}?app=1")).arg("--window-size=1000,720").arg(format!("--user-data-dir={}", profile.display())).spawn() {
                    self.web_ui = Some(c);
                    return true;
                }
            }
        }
        false
    }

    fn place(&mut self, ctx: &egui::Context, size: (f32, f32), pos: (f32, f32)) {
        ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(size.0, size.1)));
        ctx.send_viewport_cmd(ViewportCommand::OuterPosition(pos2(pos.0, pos.1)));
        self.size = size;
        self.pos = pos;
    }

    /// Ubah tinggi jendela peliharaan (untuk popup) dengan sudut kanan-bawah tetap.
    fn sync_pet_window(&mut self, ctx: &egui::Context) {
        let br = (self.pos.0 + self.size.0, self.pos.1 + self.size.1);
        let to = (W, WH + self.popup_h);
        self.place(ctx, to, ((br.0 - to.0).max(0.0), (br.1 - to.1).max(0.0)));
    }

    /// Tampilkan jawaban sebagai kartu popup di atas peliharaan (tanpa membuka obrolan).
    fn show_popup(&mut self, ctx: &egui::Context, agent: usize, text: &str) {
        self.show_popup_ex(ctx, agent, text, false)
    }

    fn show_popup_ex(&mut self, ctx: &egui::Context, agent: usize, text: &str, mono: bool) {
        let text: String = text.chars().take(1600).collect();
        let charts = text.lines().filter(|l| l.starts_with("CHART ")).count();
        let lines: usize = text.lines().filter(|l| !l.starts_with("CHART ") && !l.starts_with("IMG ")).map(|l| (l.chars().count() / if mono { 38 } else { 44 }).max(0) + 1).sum();
        let h = (64.0 + 17.0 * lines as f32 + 104.0 * charts as f32).clamp(96.0, 380.0);
        let secs = ((text.chars().count() as u64) / 12).clamp(10, 50);
        self.wheel_open = false;
        self.popup = Some(Popup { mono, agent, text, until: Instant::now() + Duration::from_secs(secs) });
        self.popup_h = h;
        self.bubble = None;
        self.sync_pet_window(ctx);
    }

    /// Bilah ketik terpisah, tetap di kanan bawah layar (tidak ikut berjalan bersama karakter); bisa dikecilkan.
    fn show_dock(&mut self, ctx: &egui::Context, srv: &Poll, mon: egui::Vec2) {
        let id = egui::ViewportId::from_hash_of("nova-dock");
        let hidden = self.dock_hidden;
        let (w, h) = if hidden { (46.0, 46.0) } else { (330.0, 50.0) };
        let pos = pos2(mon.x - w - 16.0, mon.y - h - 64.0);
        if self.dock_size != (w, h) {
            self.dock_size = (w, h);
            ctx.send_viewport_cmd_to(id, ViewportCommand::InnerSize(vec2(w, h)));
            ctx.send_viewport_cmd_to(id, ViewportCommand::OuterPosition(pos));
        }
        if self.focus_next && !hidden {
            ctx.send_viewport_cmd_to(id, ViewportCommand::Focus);
        }
        let vb = egui::ViewportBuilder::default().with_title("NOVA ketik").with_inner_size([w, h]).with_position(pos).with_decorations(false).with_transparent(true).with_always_on_top().with_taskbar(false).with_resizable(false);
        let mut toggle = false;
        ctx.show_viewport_immediate(id, vb, |ctx, _class| {
            egui::CentralPanel::default().frame(egui::Frame::none()).show(ctx, |ui| {
                if hidden {
                    let (r, resp) = ui.allocate_exact_size(vec2(46.0, 46.0), Sense::click());
                    let c = r.center();
                    ui.painter().circle_filled(c, 21.0, if resp.hovered() { Color32::from_rgb(30, 60, 110) } else { Color32::from_rgb(14, 26, 46) });
                    ui.painter().circle_stroke(c, 21.0, Stroke::new(1.6_f32, Color32::from_rgb(110, 175, 255)));
                    // ikon gelembung percakapan
                    let b = Rect::from_center_size(c + vec2(0.0, -2.0), vec2(20.0, 14.0));
                    ui.painter().rect_filled(b, 5.0, Color32::from_rgb(190, 220, 255));
                    ui.painter().add(egui::Shape::convex_polygon(vec![b.left_bottom() + vec2(4.0, -1.0), b.left_bottom() + vec2(4.0, 6.0), b.left_bottom() + vec2(11.0, -1.0)], Color32::from_rgb(190, 220, 255), Stroke::NONE));
                    if resp.on_hover_text("Buka bilah obrolan").clicked() {
                        toggle = true;
                    }
                } else {
                    let r = ui.max_rect().shrink(2.0);
                    ui.painter().rect_filled(r, 18.0, Color32::from_rgba_unmultiplied(9, 18, 32, 240));
                    ui.painter().rect_stroke(r, 18.0, Stroke::new(1.4_f32, Color32::from_rgb(110, 175, 255)));
                    ui.allocate_ui_at_rect(r.shrink2(vec2(10.0, 4.0)), |ui| {
                        ui.horizontal(|ui| {
                            if ui.add(egui::Button::new(RichText::new("—").size(14.0)).rounding(12.0).min_size(vec2(22.0, 28.0))).on_hover_text("Kecilkan bilah").clicked() {
                                toggle = true;
                            }
                            self.input_ui(ui, ctx, srv);
                        });
                    });
                }
            });
        });
        if toggle {
            self.dock_hidden = !self.dock_hidden;
        }
    }

    fn open_wheel(&mut self, ctx: &egui::Context) {
        self.popup = None;
        self.wheel_open = true;
        self.wheel_until = Instant::now() + Duration::from_secs(14);
        self.popup_h = 150.0;
        self.sync_pet_window(ctx);
    }

    fn close_wheel(&mut self, ctx: &egui::Context) {
        if self.wheel_open {
            self.wheel_open = false;
            if self.popup.is_none() {
                self.popup_h = 0.0;
                self.sync_pet_window(ctx);
            }
        }
    }

    fn close_popup(&mut self, ctx: &egui::Context) {
        if self.popup.take().is_some() {
            self.popup_h = 0.0;
            self.sync_pet_window(ctx);
        }
    }

    /// Buka/tutup obrolan. Terbuka = layar besar di tengah (seperti ChatGPT/Gemini); tertutup = kembali ke sudut peliharaan.
    fn set_chat(&mut self, ctx: &egui::Context, open: bool) {
        if open == self.chat_open {
            return;
        }
        if open && self.open_web_ui() {
            self.popup = None;
            self.popup_h = 0.0;
            self.sync_pet_window(ctx);
            return;
        }
        if open {
            self.popup = None;
            self.popup_h = 0.0;
            self.anchor = (self.pos.0 + self.size.0, self.pos.1 + self.size.1);
            let ms = ctx.input(|i| i.viewport().monitor_size).unwrap_or(vec2(1366.0, 768.0));
            // Konservatif: sisakan ruang untuk bilah tugas, bilah judul, dan penskalaan layar.
            let size = ((ms.x * 0.6).clamp(480.0, 980.0).min(ms.x - 60.0), (ms.y * 0.68).clamp(380.0, 720.0).min(ms.y - 140.0));
            self.chat_open = true;
            self.focus_next = true;
            ctx.send_viewport_cmd(ViewportCommand::Decorations(true));
            ctx.send_viewport_cmd(ViewportCommand::Resizable(true));
            ctx.send_viewport_cmd(ViewportCommand::Title("NOVA".into()));
            ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(vec2(420.0, 340.0)));
            ctx.send_viewport_cmd(ViewportCommand::Focus);
            self.place(ctx, size, ((ms.x - size.0) / 2.0, ((ms.y - size.1) / 2.0 - 30.0).max(10.0)));
            self.goto(St::Idle, 3600.0);
        } else {
            self.chat_open = false;
            ctx.send_viewport_cmd(ViewportCommand::Decorations(false));
            ctx.send_viewport_cmd(ViewportCommand::Resizable(false));
            ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(vec2(W, WH)));
            let to = (W, WH);
            self.place(ctx, to, ((self.anchor.0 - to.0).max(0.0), (self.anchor.1 - to.1).max(0.0)));
            self.goto(St::Idle, 3.0);
        }
        self.fast.store(open, Ordering::Relaxed);
    }

    fn send(&self, v: serde_json::Value) {
        let _ = self.cmd.send(v);
    }
}

struct KarAct {
    frames: Vec<egui::TextureHandle>,
    loops: u32,
    desc: String,
}

struct Kar {
    walk: Vec<egui::TextureHandle>,
    drag: Vec<egui::TextureHandle>,
    fall: Vec<egui::TextureHandle>,
    climb: Vec<egui::TextureHandle>,
    idle: Vec<KarAct>,
    special: Vec<KarAct>,
}

fn tex(ctx: &egui::Context, p: &std::path::Path, nama: &str) -> Option<egui::TextureHandle> {
    let img = image::open(p).ok()?.to_rgba8();
    let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
    Some(ctx.load_texture(format!("{nama}/{}", p.file_name()?.to_string_lossy()), ci, egui::TextureOptions::LINEAR))
}

fn load_kar(ctx: &egui::Context, nama: &str) -> Option<Kar> {
    let s = crate::karakter::muat(nama)?;
    let all = |v: &Vec<std::path::PathBuf>| -> Vec<egui::TextureHandle> { v.iter().filter_map(|p| tex(ctx, p, nama)).collect() };
    let acts = |v: &Vec<crate::karakter::Aksi>| -> Vec<KarAct> { v.iter().map(|a| KarAct { frames: a.frames.iter().filter_map(|p| tex(ctx, p, nama)).collect(), loops: a.loops, desc: a.desc.to_lowercase() }).filter(|a| !a.frames.is_empty()).collect() };
    Some(Kar { walk: all(&s.walk), drag: all(&s.drag), fall: all(&s.fall), climb: all(&s.climb), idle: acts(&s.idle), special: acts(&s.special) })
}

fn pick_frame(f: &[egui::TextureHandle], t: f64, fps: f64) -> Option<&egui::TextureHandle> {
    if f.is_empty() { None } else { Some(&f[((t * fps) as usize) % f.len()]) }
}

#[cfg(windows)]
fn lmb_down() -> bool {
    unsafe { (windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(0x01) as u16 & 0x8000) != 0 }
}

#[cfg(not(windows))]
fn lmb_down() -> bool {
    false
}

fn neg1() -> f32 {
    -1.0
}

impl eframe::App for Pet {
    fn clear_color(&self, _v: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    fn update(&mut self, ctx: &egui::Context, _f: &mut eframe::Frame) {
        // --- data dari server ---
        while let Ok(p) = self.rx.try_recv() {
            match p {
                Some(p) => {
                    self.ever = true;
                    self.fails = 0;
                    self.polled_at = Instant::now();
                    // "obrolan" diketik: buka layar obrolan besar (nilai awal tidak dihitung agar tidak terbuka saat start).
                    match self.big_seen {
                        Some(b) if b != p.open_big && !self.chat_open => self.set_chat(ctx, true),
                        _ => {}
                    }
                    self.big_seen = Some(p.open_big);
                    if p.open_chat != self.open_seen {
                        if self.open_seen != 0 || p.open_chat > 0 {
                            self.focus_next = true;
                            ctx.send_viewport_cmd(ViewportCommand::Focus);
                        }
                        self.open_seen = p.open_chat;
                    }
                    if p.approval.is_some() && !self.chat_open {
                        self.set_chat(ctx, true);
                    }
                    self.last = Some(p);
                }
                None => self.fails += 1,
            }
        }
        if let Some(p) = &self.last {
            if p.karakter != self.kar_name {
                let n = p.karakter.clone();
                self.kar = if n.is_empty() { None } else { load_kar(ctx, &n) };
                self.kar_name = n;
                self.special = None;
            }
        }
        // Tes BEC: ambil soal saat waktunya, lalu kunci layar (penuh) sampai selesai.
        while let Ok(t) = self.bec_rx.try_recv() {
            self.bec_fetching = false;
            if let Some(set) = t {
                if self.bec.is_none() {
                    self.pos_before_bec();
                    self.chat_open = false;
                    self.fast.store(false, Ordering::Relaxed);
                    ctx.send_viewport_cmd(ViewportCommand::Fullscreen(true));
                    self.bec = Some(BecRun::new(set));
                }
            } else {
                self.bec_block_until = Instant::now() + Duration::from_secs(30);
            }
        }
        if self.bec.is_none() && !self.bec_fetching && Instant::now() > self.bec_block_until && self.last.as_ref().is_some_and(|p| p.bec_due) {
            let port = self.port;
            self.start_bec_fetch(port);
        }
        if self.bec.is_some() {
            self.bec_ui(ctx);
            return;
        }
        // Server (proses tray) hilang -> ikut berhenti.
        if (self.ever && self.fails >= 15) || (!self.ever && self.started.elapsed() > Duration::from_secs(90)) {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }

        // Tombol X pada bilah judul menutup layar obrolan (kembali ke peliharaan), bukan mematikan bot.
        if self.chat_open && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.set_chat(ctx, false);
        }
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32().min(0.5);
        self.last_frame = now;
        let t = ctx.input(|i| i.time);
        let mon = ctx.input(|i| i.viewport().monitor_size).unwrap_or(vec2(1366.0, 768.0));
        if !self.placed {
            self.pos = (mon.x - W - 40.0, mon.y - WH - 48.0);
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(pos2(self.pos.0, self.pos.1)));
            self.placed = true;
            self.ground_y = self.pos.1;
        } else if let Some(r) = ctx.input(|i| i.viewport().outer_rect) {
            self.pos = (r.min.x, r.min.y);
        }

        // Jendela transparan tidak boleh menghalangi mouse/ketikan: di luar sprite, klik diteruskan ke aplikasi di bawahnya.
        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::POINT;
            use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
            let mut pt = POINT { x: 0, y: 0 };
            let ok = unsafe { GetCursorPos(&mut pt) } != 0;
            let ppp = ctx.pixels_per_point();
            let local = pos2(pt.x as f32 / ppp - self.pos.0, pt.y as f32 / ppp - self.pos.1);
            let want = ok && !self.dragging && !self.chat_open && self.bec.is_none() && self.hit != Rect::NOTHING && !self.hit.contains(local);
            if want != self.pass {
                self.pass = want;
                ctx.send_viewport_cmd(ViewportCommand::MousePassthrough(want));
            }
        }

        // Kedip (wink) selesai -> buka obrolan.
        if self.wink_until.is_some_and(|u| now > u) {
            self.wink_until = None;
            self.set_chat(ctx, true);
        }
        let srv = self.last.clone().unwrap_or_default();
        let scene = self.scene_age();

        // --- rupa: pembicara adegan, atau agen yang paling cocok dengan konteks ---
        let mut target = srv.active % agents::N;
        if let (Some(age), true) = (scene, !srv.lines.is_empty()) {
            let k = ((age / LINE_SECS) as usize).min(srv.lines.len() - 1);
            target = srv.lines[k].0 % agents::N;
            if self.shown != (srv.scene_id, k) {
                self.shown = (srv.scene_id, k);
                let text = srv.lines[k].1.clone();
                if !self.chat_open {
                    self.say(&text, LINE_SECS as u64 - 1);
                }
            }
        }
        let alert = srv.mood == "waspada" || srv.mood == "panas";
        if alert && scene.is_none() {
            target = 2;
        }
        if Animal::from_idx(target) != self.who && (scene.is_some() || alert || self.last_swap.elapsed() > Duration::from_secs(25)) {
            self.who = Animal::from_idx(target);
            self.last_swap = now;
            self.fx_until = Some(now + Duration::from_millis(700));
        }
        if alert && !self.alerting && !self.chat_open {
            self.goto(St::Interrupted, 1.8);
        }
        self.alerting = alert;
        if self.bubble.as_ref().is_some_and(|(_, until)| now > *until) {
            self.bubble = None;
        }

        // Setiap pesan Bos baru (dari mana pun) yang dijawab saat obrolan tertutup -> tampil sebagai popup.
        if let Some(last_user) = srv.chat.iter().rev().find(|l| l.who == -1).map(|l| l.seq) {
            match self.seen_user_seq {
                None => self.seen_user_seq = Some(last_user), // riwayat lama: jangan memunculkan popup
                Some(prev) if last_user > prev => {
                    self.seen_user_seq = Some(last_user);
                    if !self.chat_open && self.await_reply.is_none() {
                        self.await_reply = Some((last_user, None));
                    }
                }
                _ => {}
            }
        } else if self.seen_user_seq.is_none() && self.last.is_some() {
            self.seen_user_seq = Some(0);
        }
        let ui_open = srv.ui_age >= 0.0 && srv.ui_age < 6.0;
        // Terminal mini: keluaran perintah muncul sebagai kartu monospace di atas robot.
        if self.seen_term == u64::MAX {
            self.seen_term = srv.term_seq;
        } else if srv.term_seq != self.seen_term {
            self.seen_term = srv.term_seq;
            if !self.chat_open && !ui_open && !srv.term.is_empty() {
                self.show_popup_ex(ctx, 1, &srv.term, true);
            }
        }
        // Balasan -> popup di atas peliharaan (tanpa membuka obrolan). Bila obrolan besar sedang terbuka, balasan tampil di sana.
        if let Some((seq0, at)) = self.await_reply {
            let busy = srv.thinking || srv.approval.is_some() || srv.ask.is_some();
            let news: Vec<&ChatLine> = srv.chat.iter().filter(|l| l.who != -1 && l.seq > seq0).collect();
            match at {
                None if !news.is_empty() && !busy => self.await_reply = Some((seq0, Some(now + Duration::from_millis(1300)))),
                Some(a) if now >= a && !busy => {
                    self.await_reply = None;
                    if !self.chat_open && !news.is_empty() && !ui_open {
                        let agent = news.iter().find(|l| l.who >= 0).map(|l| l.who as usize % agents::N).unwrap_or(0);
                        let txt = news
                            .iter()
                            .map(|l| match l.text.split_once(" → ") {
                                Some((h, rest)) if h.starts_with('⚙') => rest.to_string(),
                                _ => l.text.clone(),
                            })
                            .collect::<Vec<_>>()
                            .join("\n\n");
                        self.show_popup(ctx, agent, &txt);
                    }
                }
                _ => {}
            }
        }
        // Popup menutup sendiri setelah beberapa detik (tetap terbuka selama kursor di atasnya).
        if let Some(pp) = self.popup.clone() {
            let over = ctx.input(|i| i.pointer.hover_pos()).is_some_and(|p| p.y < self.popup_h);
            if over {
                if let Some(x) = self.popup.as_mut() {
                    x.until = x.until.max(now + Duration::from_secs(4));
                }
            } else if now >= pp.until || self.chat_open {
                self.close_popup(ctx);
            }
        }
        if !self.chat_open && self.bec.is_none() && !(srv.ui_age >= 0.0 && srv.ui_age < 6.0) {
            let srv2 = srv.clone();
            self.show_dock(ctx, &srv2, mon);
        }
        if self.close_popup_flag {
            self.close_popup_flag = false;
            self.close_popup(ctx);
        }
        if self.chat_open && ctx.input(|i| i.key_pressed(Key::Escape)) && self.input.is_empty() {
            self.set_chat(ctx, false);
        }

        // Bubble ucapan dirender sebagai popup berukuran otomatis agar teks tidak terpotong.
        if !self.chat_open && self.popup.is_none() {
            if let Some((t, until)) = self.bubble.take() {
                if t.chars().count() > 40 {
                    let who = self.who.idx();
                    self.show_popup(ctx, who, &t);
                    if let Some(pp) = self.popup.as_mut() {
                        pp.until = until.max(Instant::now() + Duration::from_secs(8));
                    }
                } else {
                    self.bubble = Some((t, until));
                }
            }
        }
        // --- FSM (dibekukan saat obrolan terbuka) ---
        let mut moving = false;
        if !self.chat_open {
            let want_sleep = self.paused || (scene.is_none() && (srv.media.is_empty() && !srv.typing && (srv.user_idle >= SLEEP_AFTER_IDLE || srv.mood == "tidur")));
            match self.st {
                St::Sleep if !want_sleep => self.goto(St::Interrupted, 1.2),
                St::Sleep => {}
                _ if want_sleep => self.goto(St::Sleep, 3600.0),
                St::Interrupted if now > self.st_until => self.goto(St::Idle, 4.0),
                St::Idle if now > self.st_until && scene.is_none() && self.bubble.is_none() && self.special.is_none() => self.goto(St::Wander, 4.0 + (t as u64 % 4) as f32),
                St::Wander if now > self.st_until || scene.is_some() => self.goto(St::Idle, 5.0 + (t as u64 % 5) as f32),
                _ => {}
            }
            moving = self.st == St::Wander;
            if moving {
                let max = (mon.x - W).max(0.0);
                let mut x = self.pos.0 + self.dir * 40.0 * dt;
                if x <= 0.0 || x >= max {
                    self.dir = -self.dir;
                    x = x.clamp(0.0, max);
                    // karakter: kadang memanjat tepi layar sebelum berbalik
                    if self.kar.as_ref().is_some_and(|k| !k.climb.is_empty()) && (t as u64) % 3 == 0 {
                        self.goto(St::Climb, 5.0);
                    }
                }
                ctx.send_viewport_cmd(ViewportCommand::OuterPosition(pos2(x, self.pos.1)));
            }
        }

        // --- Karakter: seretan, jatuh, memanjat, aksi khusus ---
        if !self.chat_open {
            if self.kar.is_some() {
                let ground = self.ground_y;
                if self.dragging {
                    if !lmb_down() {
                        self.dragging = false;
                        if self.pos.1 < ground - 8.0 {
                            self.fall_vy = 0.0;
                            self.goto(St::Fall, 30.0);
                        } else {
                            self.goto(St::Idle, 2.0);
                        }
                    } else {
                        self.st_until = now + Duration::from_millis(500);
                    }
                }
                match self.st {
                    St::Fall => {
                        self.fall_vy += 1700.0 * dt;
                        let y = (self.pos.1 + self.fall_vy * dt).min(ground);
                        ctx.send_viewport_cmd(ViewportCommand::OuterPosition(pos2(self.pos.0, y)));
                        if y >= ground {
                            self.goto(St::Idle, 2.0);
                        }
                    }
                    St::Climb => {
                        let y = self.pos.1 - 75.0 * dt;
                        let top = (ground - mon.y * 0.45).max(0.0);
                        if y <= top || now > self.st_until {
                            self.fall_vy = 0.0;
                            self.goto(St::Fall, 30.0);
                        } else {
                            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(pos2(self.pos.0, y)));
                        }
                    }
                    _ => {}
                }
                if self.st == St::Idle && self.special.is_none() && now > self.next_special && scene.is_none() && !srv.thinking && self.bubble.is_none() && self.popup.is_none() && !self.wheel_open {
                    if let Some(k) = &self.kar {
                        if !k.special.is_empty() {
                            let i = (t * 1000.0) as usize % k.special.len();
                            let a = &k.special[i];
                            let dur = (a.frames.len() as f64 / 5.0 * a.loops as f64).clamp(2.0, 7.0);
                            self.special = Some((i, t, t + dur));
                        }
                    }
                    self.next_special = now + Duration::from_secs(30 + (t as u64 % 30));
                }
                if self.special.is_some_and(|(_, _, end)| t > end) || (self.special.is_some() && self.st != St::Idle) {
                    self.special = None;
                }
            } else {
                self.dragging = false;
            }
            if self.wheel_open && now > self.wheel_until {
                self.close_wheel(ctx);
            }
        }

        // Seret & lepas berkas/folder ke peliharaan -> info + tombol tindakan.
        let dropped: Vec<std::path::PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        if let Some(p) = dropped.first() {
            self.send(serde_json::json!({"Chat": format!("/berkas {}", p.display())}));
            if dropped.len() > 1 {
                self.say(&format!("{} item dilepas; kuperiksa yang pertama dulu.", dropped.len()), 6);
            }
        }
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
        let main = self.who.rgb();
        let name = agents::PERSONAS[self.who.idx()].name;
        let mut toggle_chat = false;

        if self.chat_open {
            // ------------------ Mode obrolan ------------------
            let frame = egui::Frame::none()
                .fill(Color32::from_rgb(11, 20, 36))
                .stroke(Stroke::new(1.5_f32, Color32::from_rgb(main.0, main.1, main.2)))
                .rounding(0.0)
                .inner_margin(egui::Margin::same(8.0));
            egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
                let mut run: Option<(&str, bool)> = None; // (perintah, true = kirim / false = isi kotak ketik)
                egui::menu::bar(ui, |ui| {
                    ui.menu_button("Aksi", |ui| {
                        for (l, c) in [("Bisa tidur 8 jam?", "bisa tidur 8 jam malam ini"), ("Pola & kelelahan", "/pola"), ("Cek Wi-Fi", "cek wifi"), ("Port aktif", "cek port yang aktif"), ("Baterai", "kapan baterai habis"), ("Agenda", "agenda"), ("Jadwal sholat", "/sholat")] {
                            if ui.button(l).clicked() {
                                run = Some((c, true));
                                ui.close_menu();
                            }
                        }
                    });
                    ui.menu_button("Berkas", |ui| {
                        for (l, c, send) in [("Pilih berkas…", "/pilih", true), ("Pilih folder…", "/pilih folder", true), ("Cari berkas…", "carikan file ", false)] {
                            if ui.button(l).clicked() {
                                run = Some((c, send));
                                ui.close_menu();
                            }
                        }
                    });
                    ui.menu_button("Tim", |ui| {
                        for (l, c, send) in [("Diskusi 4 agen…", "diskusi ", false), ("Berita hari ini", "berita", true), ("Kasih meme", "kasih meme", true)] {
                            if ui.button(l).clicked() {
                                run = Some((c, send));
                                ui.close_menu();
                            }
                        }
                    });
                    ui.menu_button("Gemini", |ui| {
                        for (l, c) in [("Tes koneksi", "/gemini tes"), ("Pilih model otomatis", "/gemini model auto"), ("Daftar model", "/gemini models"), ("Memori hemat token", "/gemini cache")] {
                            if ui.button(l).clicked() {
                                run = Some((c, true));
                                ui.close_menu();
                            }
                        }
                    });
                    ui.menu_button("Bantuan", |ui| {
                        for (l, c) in [("Mulai di sini (tur)", "/mulai"), ("Daftar perintah", "/bantuan")] {
                            if ui.button(l).clicked() {
                                run = Some((c, true));
                                ui.close_menu();
                            }
                        }
                    });
                });
                if let Some((c, send)) = run {
                    if send {
                        self.send(serde_json::json!({"Chat": c}));
                    } else {
                        self.input = c.to_string();
                        self.focus_next = true;
                    }
                }
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    let (r, resp) = ui.allocate_exact_size(vec2(56.0, 56.0), Sense::click_and_drag());
                    draw(ui.painter(), &self.who.frame(Frame::Idle), main, r.min, false, 3.4);
                    if resp.drag_started() {
                        ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                    }
                    ui.vertical(|ui| {
                        ui.label(RichText::new(name).strong().color(Color32::from_rgb(main.0, main.1, main.2)));
                        let sub = if srv.thinking { "berpikir…".to_string() } else { srv.mode.clone() };
                        ui.label(RichText::new(sub).small().weak());
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        if ui.small_button("×").clicked() {
                            toggle_chat = true;
                        }
                        let unread = srv.notice_seq.saturating_sub(self.notice_seen);
                        let label = if unread > 0 { format!("Notifikasi ({unread} baru)") } else { "Notifikasi".to_string() };
                        if pill(ui, &label, Color32::from_rgb(255, 214, 90), unread > 0).clicked() {
                            self.show_notices = !self.show_notices;
                            self.notice_seen = srv.notice_seq;
                        }
                    });
                });
                ui.separator();
                if self.show_notices {
                    egui::Frame::none().fill(Color32::from_rgb(30, 26, 12)).stroke(Stroke::new(1.0_f32, Color32::from_rgb(255, 214, 90))).rounding(10.0).inner_margin(egui::Margin::same(8.0)).show(ui, |ui| {
                        ui.label(RichText::new("Notifikasi (peringatan & pengingat otomatis — tidak masuk obrolan)").small().strong().color(Color32::from_rgb(255, 224, 140)));
                        egui::ScrollArea::vertical().max_height(120.0).id_salt("notices").show(ui, |ui| {
                            if srv.notices.is_empty() {
                                ui.label(RichText::new("Belum ada.").weak());
                            }
                            for n in &srv.notices {
                                ui.label(RichText::new(clean_ui(n)).size(13.0).color(Color32::from_rgb(235, 225, 190)));
                            }
                        });
                    });
                }
                let approval_h = if srv.approval.is_some() { 118.0 } else { 0.0 };
                egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false, false]).max_height(ui.available_height() - 40.0 - approval_h).show(ui, |ui| {
                    if srv.chat.is_empty() {
                        ui.label(RichText::new("Halo Bos! Ketik apa saja: 'kapan baterai habis', 'diskusi lembur vs istirahat', 'cek wifi', 'bisa tidur 8 jam?', atau /bantuan. Seret berkas ke peliharaan untuk info cepat. Esc menutup layar ini.").size(15.0).weak());
                    }
                    for l in &srv.chat {
                        match l.who {
                            -1 => {
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                                    egui::Frame::none().fill(Color32::from_rgb(34, 72, 140)).rounding(16.0).inner_margin(egui::Margin::symmetric(8.0, 4.0)).show(ui, |ui| {
                                        ui.set_max_width(((ui.ctx().screen_rect().width() - 70.0) * 0.8).max(160.0));
                                        ui.label(RichText::new(clean_ui(&l.text)).size(15.0).color(Color32::WHITE));
                                    });
                                });
                            }
                            -2 => {
                                ui.label(RichText::new(clean_ui(&l.text)).size(13.0).color(Color32::from_rgb(150, 165, 185)));
                            }
                            i => {
                                let i = i as usize % agents::N;
                                let icon = ["■", "■", "■", "■"][i % 4];
                                ui.horizontal_top(|ui| {
                                    avatar(ui, i, 2.6);
                                    let w = (ui.available_width() - 34.0).clamp(160.0, 1000.0);
                                    egui::Frame::none().fill(Color32::from_rgb(22, 32, 52)).stroke(Stroke::new(1.0_f32, Color32::from_rgb(52, 70, 104))).rounding(16.0).inner_margin(egui::Margin::symmetric(12.0, 8.0)).show(ui, |ui| {
                                        ui.set_max_width(w);
                                        ui.vertical(|ui| {
                                            ui.set_max_width(w);
                                            ui.label(RichText::new(format!("{icon} {}", agents::PERSONAS[i].name)).small().strong().color(agent_color(i)));
                                            rich(ui, &l.text, Color32::from_rgb(222, 236, 250));
                                        });
                                    });
                                });
                            }
                        }
                        ui.add_space(3.0);
                    }
                    if srv.thinking {
                        ui.horizontal(|ui| {
                            avatar(ui, self.who.idx(), 2.2);
                            ui.label(RichText::new("sedang berpikir…").italics().color(Color32::from_rgb(150, 190, 230)));
                        });
                    }
                    for j in srv.progress.iter().take(2) {
                        let (teks, warna) = if j.gagal { (format!("{} · gagal", j.judul), Color32::from_rgb(255, 120, 120)) } else if j.selesai { (format!("{} · selesai", j.judul), Color32::from_rgb(110, 210, 150)) } else { (format!("{} · {}%", j.judul, j.pct), Color32::from_rgb(125, 176, 255)) };
                        ui.add(egui::ProgressBar::new(j.pct as f32 / 100.0).fill(warna).text(teks));
                        ui.label(RichText::new(&j.label).small().weak());
                    }
                });
                if let Some(a) = srv.approval.clone() {
                    egui::Frame::none().fill(Color32::from_rgb(52, 18, 24)).stroke(Stroke::new(1.5_f32, Color32::from_rgb(255, 90, 100))).rounding(8.0).inner_margin(egui::Margin::same(6.0)).show(ui, |ui| {
                        ui.label(RichText::new(format!("⚠ Izin: {} — {}", a.tool, a.reason)).strong().color(Color32::from_rgb(255, 150, 155)));
                        egui::ScrollArea::vertical().max_height(40.0).show(ui, |ui| {
                            ui.label(RichText::new(&a.arg).monospace().small());
                        });
                        ui.horizontal(|ui| {
                            if pill(ui, "✔ Izinkan", Color32::from_rgb(90, 220, 140), true).clicked() {
                                self.send(serde_json::json!({"Approve": true}));
                            }
                            if pill(ui, "✖ Tolak", Color32::from_rgb(255, 110, 120), false).clicked() {
                                self.send(serde_json::json!({"Approve": false}));
                            }
                        });
                    });
                }
                if !srv.macros.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("⚡").small());
                        for m in srv.macros.clone() {
                            if pill(ui, &m, Color32::from_rgb(130, 180, 255), false).clicked() {
                                if m == "commit" {
                                    self.input = "/makro commit ".into();
                                    self.focus_next = true;
                                } else {
                                    self.send(serde_json::json!({"Chat": format!("/makro {m}")}));
                                }
                            }
                        }
                        if pill(ui, "port", Color32::from_rgb(130, 180, 255), false).clicked() {
                            self.send(serde_json::json!({"Chat": "/port"}));
                        }
                    });
                }
                if let Some(a) = srv.ask.clone() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(format!("❓ {}", a.question)).small().strong().color(Color32::from_rgb(255, 214, 90)));
                        for (oi, o) in a.options.iter().enumerate() {
                            if pill(ui, o, Color32::from_rgb(255, 214, 90), oi == 0).clicked() {
                                self.send(serde_json::json!({"Chat": o}));
                            }
                        }
                    });
                }
                if srv.feedback {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Tindakanku tadi:").small().weak());
                        if pill(ui, "👍 cocok", Color32::from_rgb(90, 220, 140), false).clicked() {
                            self.send(serde_json::json!({"Feedback": true}));
                        }
                        if pill(ui, "👎 mengganggu", Color32::from_rgb(255, 110, 120), false).clicked() {
                            self.send(serde_json::json!({"Feedback": false}));
                        }
                    });
                }
                self.input_ui(ui, ctx, &srv);
            });
        } else {
            // ------------------ Mode peliharaan ------------------
            egui::CentralPanel::default().frame(egui::Frame::none()).show(ctx, |ui| {
                let hop = if self.st == St::Interrupted { -((t * 14.0).sin().abs() as f32) * 14.0 } else { 0.0 };
                let bob = if self.st == St::Idle && (t * 2.0) as u64 % 2 == 0 { 0.0 } else { SCALE * 0.6 };
                let shake = if alert { ((t * 20.0).sin() as f32) * 3.0 } else { 0.0 };
                let act_on = srv.act_age >= 0.0 && (srv.act_age + self.polled_at.elapsed().as_secs_f32()) * 1000.0 < srv.act_ms as f32;
                let amb: &str = if matches!(self.st, St::Idle | St::Wander) && !act_on && !self.paused && scene.is_none() {
                    if srv.typing { "ketik" } else if srv.media == "video" { "video" } else if srv.media == "musik" { "musik" } else { "" }
                } else {
                    ""
                };
                // mata/kepala mengikuti kursor (hanya saat diam) + mengangguk mengikuti musik
                let gaze = if matches!(self.st, St::Idle) && !act_on { gaze_dx(ctx, W) } else { 0.0 };
                let nod = if amb == "musik" { ((t * 5.0).sin() as f32) * 3.0 } else { 0.0 };
                let oy = self.popup_h;
                let origin = pos2((W - 16.0 * SCALE) / 2.0 + shake + gaze, oy + H - 16.0 * SCALE - 14.0 + nod + if self.st == St::Sleep { 0.0 } else { bob + hop });
                let rect = Rect::from_min_size(origin, vec2(16.0 * SCALE, 16.0 * SCALE));
                self.hit = rect.expand(14.0);
                let frame = match self.st {
                    St::Sleep => Frame::Sleep,
                    St::Wander => {
                        if (t * 4.0) as u64 % 2 == 0 { Frame::WalkA } else { Frame::WalkB }
                    }
                    _ => Frame::Idle,
                };
                let p = ui.painter();
                if let Some(k) = self.kar.as_ref() {
                    let mut flip = self.dir > 0.0;
                    let tf: Option<&egui::TextureHandle> = match self.st {
                        St::Wander => pick_frame(&k.walk, t, 7.0),
                        St::Interrupted => pick_frame(&k.drag, t, 9.0).or(k.walk.first()),
                        St::Fall => pick_frame(&k.fall, t, 9.0).or(k.walk.first()),
                        St::Climb => {
                            flip = self.pos.0 > 5.0;
                            pick_frame(&k.climb, t, 6.0).or(k.walk.first())
                        }
                        St::Sleep => k.idle.iter().find(|a| a.desc.contains("sit") || a.desc.contains("sleep")).or(k.idle.last()).and_then(|a| a.frames.first()).or(k.walk.first()),
                        St::Idle => {
                            if let Some((i, t0, _)) = self.special {
                                k.special.get(i).and_then(|a| pick_frame(&a.frames, t - t0, 5.0))
                            } else if srv.thinking {
                                k.idle.iter().find(|a| a.desc.contains("think")).or(k.idle.first()).and_then(|a| pick_frame(&a.frames, t, 3.0))
                            } else {
                                k.idle.first().and_then(|a| pick_frame(&a.frames, t, 3.0)).or(k.walk.first())
                            }
                        }
                    };
                    if let Some(tx) = tf {
                        let sz = 104.0;
                        let tr = Rect::from_min_size(pos2(origin.x + (16.0 * SCALE - sz) / 2.0, rect.bottom() - sz + 6.0), vec2(sz, sz));
                        let uv = if flip { Rect::from_min_max(pos2(1.0, 0.0), pos2(0.0, 1.0)) } else { Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)) };
                        p.image(tx.id(), tr, uv, Color32::WHITE);
                        if !srv.topi.is_empty() {
                            draw_hat(p, &srv.topi, pos2(tr.center().x + if flip { 3.0 } else { -3.0 }, tr.top() + 15.0), 1.1);
                        }
                    }
                } else {
                    draw(p, &self.who.frame(frame), main, origin, self.dir > 0.0, SCALE);
                    if !srv.topi.is_empty() {
                        draw_hat(p, &srv.topi, pos2(origin.x + 8.0 * SCALE, origin.y + 1.0 * SCALE), 1.4);
                    }
                }
                // Animasi aksi (mencari, menembak iklan, membaca, dst.)
                let act_age = srv.act_age + self.polled_at.elapsed().as_secs_f32();
                if srv.act_age >= 0.0 && !srv.act.is_empty() && act_age * 1000.0 < srv.act_ms as f32 {
                    draw_act(p, &srv.act, &srv.act_text, origin, act_age, t as f32, main);
                }
                if self.wink_until.is_some() {
                    // Wink: mata kiri ditutup jadi garis
                    let ink = Color32::from_rgb(14, 20, 30);
                    p.rect_filled(Rect::from_min_size(px_at(origin, 5.0, 6.0), vec2(2.0 * SCALE, 2.0 * SCALE)), 0.0, ink);
                    p.line_segment([px_at(origin, 4.8, 7.0), px_at(origin, 7.0, 7.0)], Stroke::new(2.0_f32, Color32::from_rgb(225, 255, 255)));
                }
                if self.kar.is_none() && srv.emo_age >= 0.0 && srv.emo_age < 25.0 && self.st != St::Sleep {
                    expression(p, &srv.emo, origin, t);
                }
                if let Some(until) = self.fx_until {
                    if now < until {
                        let k = 1.0 - (until - now).as_secs_f32() / 0.7;
                        for i in 0..10 {
                            let a = i as f32 * 0.628 + k;
                            let r = 18.0 + k * 46.0;
                            let c = origin + vec2(40.0 + a.cos() * r, 40.0 + a.sin() * r * 0.8);
                            p.rect_filled(Rect::from_center_size(c, vec2(5.0, 5.0)), 0.0, Color32::from_rgba_unmultiplied(main.0, main.1, main.2, (255.0 * (1.0 - k)) as u8));
                        }
                    } else {
                        self.fx_until = None;
                    }
                }
                if !amb.is_empty() {
                    draw_ambient(p, amb, origin, t as f32, main);
                }
                match self.st {
                    St::Sleep => {
                        let z = ((t * 1.5) as u64 % 3) as usize;
                        p.text(pos2(origin.x + 70.0 + z as f32 * 6.0, origin.y + 6.0 - z as f32 * 8.0), Align2::LEFT_CENTER, "z", FontId::proportional(14.0 + z as f32 * 3.0), Color32::from_rgb(160, 210, 255));
                    }
                    St::Interrupted => {
                        p.text(pos2(origin.x + 40.0, origin.y - 6.0), Align2::CENTER_CENTER, "!", FontId::proportional(22.0), Color32::from_rgb(255, 214, 90));
                    }
                    _ => {}
                }
                let tag = if self.st == St::Sleep { "tidur".to_string() } else { srv.mode.clone() };
                let label = if tag.is_empty() { name.to_string() } else { format!("{name} · {tag}") };
                p.text(pos2(W / 2.0, oy + H - 2.0), Align2::CENTER_BOTTOM, label, FontId::proportional(10.0), Color32::from_rgba_unmultiplied(210, 235, 255, 200));
                if let Some((text, _)) = self.bubble.as_ref().filter(|_| self.popup.is_none()) {
                    let g = p.layout(text.clone(), FontId::proportional(12.0), Color32::from_rgb(225, 245, 255), W - 24.0);
                    let size = g.size() + vec2(16.0, 12.0);
                    let br = Rect::from_min_size(pos2((W - size.x) / 2.0, (origin.y - size.y - 4.0).max(2.0)), size);
                    p.rect_filled(br, 4.0, Color32::from_rgba_unmultiplied(16, 19, 26, 245));
                    p.rect_stroke(br, 4.0, Stroke::new(1.5_f32, Color32::from_rgb(main.0, main.1, main.2)));
                    p.galley(br.min + vec2(8.0, 6.0), g, Color32::WHITE);
                }
                // Equalizer: batang bergerak mengikuti level audio keluar (musik/video sedang bersuara).
                crate::ambien::start();
                if !crate::ambien::media().is_empty() {
                    let lv = crate::ambien::level().min(1.0);
                    let tt = ctx.input(|i| i.time) as f32;
                    let x0 = if rect.right() + 44.0 < W { rect.right() + 8.0 } else { rect.left() - 44.0 };
                    for i in 0..6 {
                        let h = 4.0 + 46.0 * lv * (0.3 + 0.7 * (tt * (5.0 + i as f32 * 1.9) + i as f32 * 1.3).sin().abs());
                        let r = Rect::from_min_max(pos2(x0 + i as f32 * 6.0, rect.bottom() - h), pos2(x0 + i as f32 * 6.0 + 4.0, rect.bottom()));
                        ui.painter().rect_filled(r, 2.0, Color32::from_rgb(110 + i as u8 * 18, 180, 255));
                    }
                    ctx.request_repaint();
                }
                let resp = ui.allocate_rect(rect, Sense::click_and_drag());
                if resp.drag_started() {
                    self.goto(St::Interrupted, 1.0);
                    self.dragging = true;
                    self.special = None;
                    ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                }
                if resp.clicked() {
                    self.paused = false;
                    // klik membuka/menutup roda keterampilan
                    if self.wheel_open { self.close_wheel(ctx) } else { self.open_wheel(ctx) }
                }
                if resp.secondary_clicked() {
                    self.paused = !self.paused;
                    self.say(if self.paused { "Zzz... (klik kanan lagi untuk bangunkan)" } else { "Siap!" }, 4);
                }
                if hovering {
                    let r = Rect::from_min_size(pos2(4.0, 4.0), vec2(W - 8.0, WH + oy - 8.0));
                    ui.painter().rect_filled(r, 16.0, Color32::from_rgba_unmultiplied(80, 150, 255, 40));
                    ui.painter().rect_stroke(r, 16.0, Stroke::new(2.5_f32, Color32::from_rgb(120, 190, 255)));
                    ui.painter().text(r.center_top() + vec2(0.0, 22.0), Align2::CENTER_CENTER, "Lepas di sini: kuperiksa", FontId::proportional(14.0), Color32::from_rgb(200, 230, 255));
                }
                if self.wheel_open && self.popup.is_none() {
                    // Roda melingkar: 8 tombol bulat di busur atas kepala karakter, ikon digambar vektor (tanpa emoji/teks panjang).
                    let center = pos2(W / 2.0, oy + 58.0);
                    let items: [(&str, &str); 8] = [("Obrolan", "@chat"), ("Berita", "berita"), ("Cuaca", "cuaca"), ("Sholat", "/sholat"), ("Pola kerja", "/pola"), ("Tidur", "bisa tidur 8 jam malam ini"), ("Catatan", "catatan"), ("Sahabat", "/buddy")];
                    ui.painter().circle_filled(center + vec2(0.0, 6.0), 118.0, Color32::from_rgba_unmultiplied(9, 18, 32, 70));
                    let mut chosen: Option<&str> = None;
                    let mut hover_label: Option<(&str, egui::Pos2)> = None;
                    for (i, (label, cmd)) in items.iter().enumerate() {
                        let a = std::f32::consts::PI * (1.06 + 0.88 * i as f32 / 7.0);
                        let c = center + vec2(a.cos() * 108.0, a.sin() * 84.0);
                        let r = Rect::from_center_size(c, vec2(40.0, 40.0));
                        let resp = ui.interact(r, ui.id().with(("roda", i)), Sense::click());
                        let hot = resp.hovered();
                        let p = ui.painter();
                        p.circle_filled(c, 20.0, if hot { Color32::from_rgb(34, 70, 128) } else { Color32::from_rgb(15, 27, 48) });
                        p.circle_stroke(c, 20.0, Stroke::new(if hot { 2.2 } else { 1.4_f32 }, Color32::from_rgb(110, 175, 255)));
                        wheel_icon(p, i, c, if hot { Color32::WHITE } else { Color32::from_rgb(170, 205, 245) });
                        if hot {
                            hover_label = Some((label, c));
                        }
                        if resp.clicked() {
                            chosen = Some(cmd);
                        }
                    }
                    if let Some((l, c)) = hover_label {
                        ui.painter().text(center + vec2(0.0, -6.0) + (c - center) * 0.0, Align2::CENTER_CENTER, l, FontId::proportional(13.0), Color32::from_rgb(220, 238, 255));
                    }
                    if let Some(c) = chosen {
                        self.close_wheel(ctx);
                        if c == "@chat" {
                            toggle_chat = true;
                        } else {
                            self.await_reply = Some((srv.chat.last().map(|l| l.seq).unwrap_or(0), None));
                            self.send(serde_json::json!({"Chat": c}));
                        }
                    }
                }
                if let Some(pp) = self.popup.clone() {
                    let area = Rect::from_min_size(pos2(6.0, 4.0), vec2(W - 12.0, oy - 8.0));
                    let col = agent_color(pp.agent);
                    ui.painter().rect_filled(area, 14.0, Color32::from_rgba_unmultiplied(9, 18, 32, 250));
                    ui.painter().rect_stroke(area, 14.0, Stroke::new(1.6_f32, col));
                    ui.allocate_ui_at_rect(area.shrink2(vec2(12.0, 8.0)), |ui| {
                        ui.horizontal(|ui| {
                            avatar(ui, pp.agent, 1.8);
                            ui.label(RichText::new(format!("{} {}", ["■", "■", "■", "■"][pp.agent % 4], agents::PERSONAS[pp.agent % agents::N].name)).small().strong().color(col));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.add(egui::Button::new("×").rounding(10.0).min_size(vec2(22.0, 20.0))).clicked() {
                                    self.close_popup_flag = true;
                                }
                                if pill(ui, "Obrolan", Color32::from_rgb(130, 180, 255), false).clicked() {
                                    toggle_chat = true;
                                }
                            });
                        });
                        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                            if pp.mono {
                                ui.add(egui::Label::new(RichText::new(&pp.text).monospace().size(11.5).color(Color32::from_rgb(120, 235, 150))).wrap());
                            } else {
                                rich(ui, &pp.text, Color32::from_rgb(222, 236, 250));
                            }
                        });
                    });
                }
                // Pertanyaan/saran NOVA: kartu kecil dengan tombol, tanpa membuka obrolan.
                if let Some(a) = srv.ask.clone() {
                    let area = Rect::from_min_size(pos2(8.0, oy + 2.0), vec2(W - 16.0, if a.options.len() > 2 { 92.0 } else { 64.0 }));
                    ui.painter().rect_filled(area, 14.0, Color32::from_rgba_unmultiplied(9, 18, 32, 245));
                    ui.painter().rect_stroke(area, 14.0, Stroke::new(1.4_f32, Color32::from_rgb(255, 214, 90)));
                    ui.allocate_ui_at_rect(area.shrink2(vec2(10.0, 6.0)), |ui| {
                        ui.label(RichText::new(format!("💡 {}", a.question)).small().strong().color(Color32::from_rgb(255, 224, 140)));
                        ui.horizontal_wrapped(|ui| {
                            for (oi, o) in a.options.iter().enumerate() {
                                if pill(ui, o, Color32::from_rgb(255, 214, 90), oi == 0).clicked() {
                                    self.await_reply = Some((srv.chat.last().map(|l| l.seq).unwrap_or(0), None));
                                    self.send(serde_json::json!({"Chat": o}));
                                }
                            }
                        });
                    });
                }
            });
        }
        if toggle_chat {
            let open = !self.chat_open;
            self.set_chat(ctx, open);
        }

        let act_busy = self.last.as_ref().is_some_and(|p| p.act_age >= 0.0 && (p.act_age + self.polled_at.elapsed().as_secs_f32()) * 1000.0 < p.act_ms as f32);
        let busy = act_busy || moving || self.bubble.is_some() || alert || scene.is_some() || self.st == St::Interrupted || self.fx_until.is_some() || self.chat_open;
        ctx.request_repaint_after(Duration::from_millis(if self.chat_open { 120 } else if busy { 80 } else { 350 }));
    }
}

// ---------- Layar kunci tes BEC ----------

#[derive(Clone, Copy, PartialEq)]
enum BecPhase {
    Intro,
    Quiz,
    Result,
    Emergency,
}

struct BecRun {
    set: bec::TestSet,
    items: Vec<bec::Item>,
    idx: usize,
    picked: Option<usize>,
    answers: Vec<bec::Ans>,
    phase: BecPhase,
    plays_left: u8,
    played_idx: usize,
    audio: Option<std::process::Child>,
    eval: String,
    emergency: String,
    last_focus: Instant,
}

impl BecRun {
    fn new(set: bec::TestSet) -> Self {
        let items: Vec<bec::Item> = set.listening.iter().chain(&set.reading).chain(&set.grammar).cloned().collect();
        Self { set, items, idx: 0, picked: None, answers: vec![], phase: BecPhase::Intro, plays_left: 0, played_idx: usize::MAX, audio: None, eval: String::new(), emergency: String::new(), last_focus: Instant::now() }
    }
    fn stop_audio(&mut self) {
        if let Some(mut c) = self.audio.take() {
            let _ = c.kill();
        }
    }
    fn play(&mut self) {
        if self.plays_left == 0 || !self.set.tts_ok {
            return;
        }
        self.stop_audio();
        if let Some(it) = self.items.get(self.idx) {
            self.audio = bec::speak(&it.audio);
            self.plays_left -= 1;
        }
    }
}

enum BecAct {
    None,
    Snooze,
    Skip,
    Done,
}

impl Pet {
    fn start_bec_fetch(&mut self, port_hint: u16) {
        self.bec_fetching = true;
        let tx = self.bec_tx.clone();
        std::thread::spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() else {
                let _ = tx.send(None);
                return;
            };
            let http = reqwest::Client::builder().timeout(Duration::from_secs(10)).build().unwrap();
            let url = format!("http://127.0.0.1:{port_hint}/api/bec");
            let r = rt.block_on(async { http.get(&url).header("x-nova-token", token()).send().await?.error_for_status()?.json::<bec::TestSet>().await });
            let _ = tx.send(r.ok());
        });
    }

    fn leave_bec(&mut self, ctx: &egui::Context) {
        if let Some(mut r) = self.bec.take() {
            r.stop_audio();
        }
        ctx.send_viewport_cmd(ViewportCommand::Fullscreen(false));
        ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(W, WH)));
        ctx.send_viewport_cmd(ViewportCommand::OuterPosition(pos2(self.pos.0, self.pos.1)));
        self.bec_block_until = Instant::now() + Duration::from_secs(90);
        self.goto(St::Interrupted, 1.5);
    }

    /// Menggambar layar kunci. Mengembalikan aksi yang harus dijalankan setelah frame.
    fn bec_ui(&mut self, ctx: &egui::Context) {
        let Some(mut run) = self.bec.take() else { return };
        // pertahankan fokus & selalu di atas
        if run.last_focus.elapsed() > Duration::from_secs(4) {
            ctx.send_viewport_cmd(ViewportCommand::Focus);
            run.last_focus = Instant::now();
        }
        let mut act = BecAct::None;
        let accent = Color32::from_rgb(90, 210, 255);
        let frame = egui::Frame::none().fill(Color32::from_rgb(7, 14, 26)).inner_margin(egui::Margin::symmetric(60.0, 36.0));
        // Pintasan keyboard: 1-4 / A-D memilih jawaban, Enter = lanjut.
        let keys = ctx.input(|i| {
            [Key::Num1, Key::Num2, Key::Num3, Key::Num4].iter().position(|k| i.key_pressed(*k)).or_else(|| [Key::A, Key::B, Key::C, Key::D].iter().position(|k| i.key_pressed(*k)))
        });
        let enter = ctx.input(|i| i.key_pressed(Key::Enter));
        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.set_max_width(980.0);
                ui.label(RichText::new("📘 BEC English Test — selesaikan untuk membuka laptop").size(26.0).strong().color(accent));
            });
            ui.add_space(8.0);
            match run.phase {
                BecPhase::Intro => {
                    ui.add_space(30.0);
                    let n = run.items.len();
                    ui.label(RichText::new(format!("{} soal: Listening {} · Reading {} · Grammar {}", n, run.set.listening.len(), run.set.reading.len(), run.set.grammar.len())).size(20.0));
                    ui.label(RichText::new(format!("Level {} · streak {} hari. Aku akan memeriksa hasilnya dan menunjukkan di mana kesulitanmu.", run.set.level, run.set.streak)).size(16.0).weak());
                    ui.add_space(6.0);
                    ui.label(RichText::new(if run.set.tts_ok { "🔊 Listening diputar dengan suara Inggris (maksimal 2x per soal). Pastikan volume menyala." } else { "ℹ Suara Inggris Windows tidak terpasang: transkrip Listening ditampilkan langsung." }).size(16.0));
                    ui.add_space(24.0);
                    ui.horizontal(|ui| {
                        if ui.add_sized([220.0, 44.0], egui::Button::new(RichText::new("▶ Mulai tes").size(20.0))).clicked() {
                            run.phase = BecPhase::Quiz;
                        }
                        if run.set.snooze_ok && ui.add_sized([200.0, 44.0], egui::Button::new(RichText::new("⏳ Tunda 10 menit").size(16.0))).clicked() {
                            act = BecAct::Snooze;
                        }
                        if ui.add_sized([200.0, 44.0], egui::Button::new(RichText::new("🚨 Darurat").size(16.0))).clicked() && run.set.skip_ok {
                            run.phase = BecPhase::Emergency;
                        }
                    });
                    if !run.set.skip_ok {
                        ui.label(RichText::new("Jalur darurat sudah dipakai 2x minggu ini.").small().weak());
                    }
                }
                BecPhase::Emergency => {
                    ui.add_space(30.0);
                    ui.label(RichText::new("Jalur darurat — hanya untuk keadaan yang benar-benar mendesak. Ketik persis: SAYA SEDANG DARURAT").size(18.0));
                    ui.add(egui::TextEdit::singleline(&mut run.emergency).desired_width(420.0));
                    ui.horizontal(|ui| {
                        if ui.add_enabled(run.emergency.trim() == "SAYA SEDANG DARURAT", egui::Button::new("Lewati tes (dicatat)")).clicked() {
                            act = BecAct::Skip;
                        }
                        if ui.button("Kembali").clicked() {
                            run.phase = BecPhase::Intro;
                        }
                    });
                }
                BecPhase::Quiz => {
                    let total = run.items.len();
                    let it = run.items[run.idx].clone();
                    // Autoplay sekali per soal listening
                    if it.section == "listening" && run.played_idx != run.idx {
                        run.played_idx = run.idx;
                        run.plays_left = 2;
                        run.play();
                    }
                    let (sec_label, sec_start, sec_n) = match it.section.as_str() {
                        "listening" => ("Listening", 0, run.set.listening.len()),
                        "reading" => ("Reading", run.set.listening.len(), run.set.reading.len()),
                        _ => ("Grammar", run.set.listening.len() + run.set.reading.len(), run.set.grammar.len()),
                    };
                    ui.label(RichText::new(format!("{sec_label} {}/{sec_n}   ·   total {}/{total}", run.idx - sec_start + 1, run.idx + 1)).size(18.0).color(accent));
                    ui.add(egui::ProgressBar::new(run.idx as f32 / total as f32).desired_width(ui.available_width().min(980.0)));
                    ui.add_space(14.0);
                    let answered = run.picked.is_some();
                    if it.section == "listening" {
                        ui.horizontal(|ui| {
                            if run.set.tts_ok {
                                let label = format!("🔊 Putar ulang ({} tersisa)", run.plays_left);
                                if ui.add_enabled(run.plays_left > 0, egui::Button::new(RichText::new(label).size(16.0))).clicked() {
                                    run.play();
                                }
                            }
                        });
                        if answered || !run.set.tts_ok {
                            egui::Frame::none().fill(Color32::from_rgb(20, 32, 52)).rounding(8.0).inner_margin(egui::Margin::same(10.0)).show(ui, |ui| {
                                ui.label(RichText::new(format!("Transkrip: {}", it.audio)).size(15.0).weak());
                            });
                        }
                    } else if !it.passage.is_empty() {
                        egui::Frame::none().fill(Color32::from_rgb(20, 32, 52)).rounding(8.0).inner_margin(egui::Margin::same(12.0)).show(ui, |ui| {
                            ui.label(RichText::new(&it.passage).size(18.0));
                        });
                    }
                    ui.add_space(10.0);
                    ui.label(RichText::new(&it.q).size(22.0).strong());
                    ui.add_space(10.0);
                    for (i, o) in it.opts.iter().enumerate() {
                        let letter = ["A", "B", "C", "D"][i % 4];
                        let mut text = RichText::new(format!("{letter}.  {o}")).size(19.0);
                        let mut btn = egui::Button::new(text.clone());
                        if let Some(p) = run.picked {
                            if i == it.ans {
                                text = text.color(Color32::from_rgb(120, 240, 150));
                                btn = egui::Button::new(text).fill(Color32::from_rgb(20, 70, 40));
                            } else if i == p {
                                text = text.color(Color32::from_rgb(255, 130, 130));
                                btn = egui::Button::new(text).fill(Color32::from_rgb(80, 24, 30));
                            }
                        }
                        let clicked = ui.add_sized([ui.available_width().min(900.0), 40.0], btn).clicked();
                        if !answered && (clicked || keys == Some(i)) {
                            run.picked = Some(i);
                            run.answers.push(bec::Ans { id: it.id.clone(), section: it.section.clone(), tag: it.tag.clone(), ok: i == it.ans });
                            run.stop_audio();
                        }
                    }
                    if let Some(p) = run.picked {
                        ui.add_space(10.0);
                        let ok = p == it.ans;
                        ui.label(RichText::new(if ok { "✅ Benar" } else { "❌ Kurang tepat" }).size(20.0).strong().color(if ok { Color32::from_rgb(120, 240, 150) } else { Color32::from_rgb(255, 130, 130) }));
                        ui.label(RichText::new(format!("💡 {}", it.why)).size(16.0));
                        ui.add_space(8.0);
                        if ui.add_sized([220.0, 40.0], egui::Button::new(RichText::new(if run.idx + 1 == total { "Lihat hasil ▶" } else { "Lanjut ▶ (Enter)" }).size(18.0))).clicked() || enter {
                            run.stop_audio();
                            if run.idx + 1 == total {
                                run.eval = bec::evaluate(&run.answers);
                                run.phase = BecPhase::Result;
                            } else {
                                run.idx += 1;
                                run.picked = None;
                            }
                        }
                    }
                }
                BecPhase::Result => {
                    let ok = run.answers.iter().filter(|a| a.ok).count();
                    ui.add_space(10.0);
                    ui.label(RichText::new(format!("Selesai! Skor total {ok}/{}", run.answers.len())).size(24.0).strong().color(accent));
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical().max_height(ui.available_height() - 80.0).show(ui, |ui| {
                        ui.label(RichText::new(&run.eval).size(17.0));
                    });
                    ui.add_space(8.0);
                    if ui.add_sized([320.0, 46.0], egui::Button::new(RichText::new("✅ Selesai — buka laptop").size(20.0))).clicked() {
                        act = BecAct::Done;
                    }
                }
            }
        });
        match act {
            BecAct::None => self.bec = Some(run),
            BecAct::Snooze => {
                self.send(serde_json::json!("BecSnooze"));
                self.bec = Some(run);
                self.leave_bec(ctx);
            }
            BecAct::Skip => {
                self.send(serde_json::json!("BecSkip"));
                self.bec = Some(run);
                self.leave_bec(ctx);
            }
            BecAct::Done => {
                self.send(serde_json::json!({"BecDone": {"answers": run.answers}}));
                self.bec = Some(BecRun::new(run.set.clone()));
                self.leave_bec(ctx);
            }
        }
        ctx.request_repaint_after(Duration::from_millis(150));
    }
}
