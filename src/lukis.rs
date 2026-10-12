//! Fungsi menggambar murni untuk robot/karakter (tanpa status): ikon, topi, ekspresi, animasi aksi, grafik mini, gaya egui.
use crate::agents::{self, pixel_rgb, Animal, Frame};
use crate::pet::SCALE;
use eframe::egui::{self, pos2, vec2, Align2, Color32, FontId, Rect, RichText, Sense, Stroke};

/// Gambar bot (sprite piksel) sebagai avatar agen.
pub(crate) fn avatar(ui: &mut egui::Ui, agent: usize, scale: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(16.0 * scale, 16.0 * scale), Sense::hover());
    let pr = &agents::PERSONAS[agent % agents::N];
    draw(ui.painter(), &pr.animal.frame(Frame::Idle), pr.rgb, rect.min, false, scale);
}

/// Grafik garis mini dari {"t": judul, "u": satuan, "v": [angka]} (sparkline dengan area).
pub(crate) fn chart(ui: &mut egui::Ui, v: &serde_json::Value) {
    let vals: Vec<f32> = v["v"].as_array().map(|a| a.iter().filter_map(|x| x.as_f64().map(|f| f as f32)).collect()).unwrap_or_default();
    if vals.len() < 2 {
        return;
    }
    let (mn, mx) = vals.iter().fold((f32::MAX, f32::MIN), |a, x| (a.0.min(*x), a.1.max(*x)));
    let w = ui.available_width().clamp(160.0, 330.0);
    let (rect, _) = ui.allocate_exact_size(vec2(w, 96.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 8.0, Color32::from_rgb(16, 26, 46));
    p.text(rect.min + vec2(8.0, 5.0), Align2::LEFT_TOP, v["t"].as_str().unwrap_or(""), FontId::proportional(11.0), Color32::from_rgb(160, 180, 210));
    let last = *vals.last().unwrap();
    let up = last >= vals[0];
    let col = if up { Color32::from_rgb(52, 211, 153) } else { Color32::from_rgb(248, 113, 113) };
    p.text(rect.right_top() + vec2(-8.0, 5.0), Align2::RIGHT_TOP, format!("{last:.2} {}", v["u"].as_str().unwrap_or("")), FontId::proportional(11.0), col);
    let area = Rect::from_min_max(rect.min + vec2(8.0, 24.0), rect.max - vec2(8.0, 8.0));
    let pts: Vec<egui::Pos2> = vals.iter().enumerate().map(|(i, x)| pos2(area.left() + area.width() * i as f32 / (vals.len() - 1) as f32, area.bottom() - area.height() * if mx > mn { (x - mn) / (mx - mn) } else { 0.5 })).collect();
    p.add(egui::Shape::line(pts, Stroke::new(1.8_f32, col)));
    p.text(area.left_bottom() + vec2(0.0, 0.0), Align2::LEFT_BOTTOM, format!("{mn:.1}"), FontId::proportional(9.0), Color32::from_rgb(110, 130, 160));
    p.text(area.left_top(), Align2::LEFT_TOP, format!("{mx:.1}"), FontId::proportional(9.0), Color32::from_rgb(110, 130, 160));
}

/// Teks bergaya obrolan: baris yang berisi URL ditampilkan sebagai tautan yang bisa diklik.
pub(crate) fn rich(ui: &mut egui::Ui, text: &str, color: Color32) {
    for line in clean_ui(text).lines() {
        let l = line.trim();
        if l.starts_with("IMG http") {
            continue; // thumbnail hanya ditampilkan di antarmuka web
        }
        if let Some(j) = l.strip_prefix("CHART ") {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(j) {
                chart(ui, &v);
            }
            continue;
        }
        if l.starts_with("http://") || l.starts_with("https://") {
            ui.add(egui::Hyperlink::from_label_and_url(RichText::new("🔗 Buka tautan").size(13.0), l)).on_hover_text(l);
        } else {
            ui.label(RichText::new(line).size(15.0).color(color));
        }
    }
}

/// Ganti karakter yang tidak ada di font bawaan egui agar tidak tampil sebagai kotak.
pub(crate) fn clean_ui(t: &str) -> String {
    t.replace('→', "->").replace('🗣', "").replace('✦', "*").replace('✎', "*")
}

/// Animasi suasana: mengetik (laptop), musik (headphone + not), video (layar kecil).
pub(crate) fn draw_ambient(p: &egui::Painter, kind: &str, o: egui::Pos2, t: f32, main: (u8, u8, u8)) {
    let c = |x: f32, y: f32| o + vec2(x, y);
    let body = Color32::from_rgb(main.0, main.1, main.2);
    let steel = Color32::from_rgb(86, 94, 112);
    match kind {
        "ketik" => {
            p.rect_filled(Rect::from_min_size(c(18.0, 40.0), vec2(44.0, 22.0)), 2.0, Color32::from_rgb(14, 22, 36));
            p.rect_stroke(Rect::from_min_size(c(18.0, 40.0), vec2(44.0, 22.0)), 2.0, Stroke::new(1.5_f32, steel));
            for i in 0..3 {
                let w = 10.0 + (((t * 4.0) as i32 + i * 7) % 5) as f32 * 5.0;
                p.rect_filled(Rect::from_min_size(c(22.0, 44.0 + i as f32 * 5.0), vec2(w, 2.0)), 0.0, Color32::from_rgb(110, 230, 150));
            }
            if (t * 3.0) as i32 % 2 == 0 {
                p.rect_filled(Rect::from_min_size(c(56.0, 54.0), vec2(2.0, 5.0)), 0.0, Color32::WHITE);
            }
            p.rect_filled(Rect::from_min_size(c(10.0, 62.0), vec2(60.0, 6.0)), 2.0, steel);
            let (a, b) = if (t * 12.0) as i32 % 2 == 0 { (0.0, 3.0) } else { (3.0, 0.0) };
            p.rect_filled(Rect::from_min_size(c(24.0, 58.0 + a), vec2(7.0, 6.0)), 2.0, body);
            p.rect_filled(Rect::from_min_size(c(46.0, 58.0 + b), vec2(7.0, 6.0)), 2.0, body);
        }
        "musik" => {
            let cx = c(40.0, 34.0);
            let pts: Vec<egui::Pos2> = (0..=12).map(|i| {
                let a = std::f32::consts::PI * (1.0 + i as f32 / 12.0);
                cx + vec2(a.cos() * 31.0, a.sin() * 30.0)
            }).collect();
            p.add(egui::Shape::line(pts, Stroke::new(3.0_f32, steel)));
            p.rect_filled(Rect::from_min_size(c(5.0, 30.0), vec2(9.0, 16.0)), 3.0, body);
            p.rect_filled(Rect::from_min_size(c(66.0, 30.0), vec2(9.0, 16.0)), 3.0, body);
            for i in 0..3 {
                let ph = (t * 0.7 + i as f32 / 3.0) % 1.0;
                let x = 78.0 + i as f32 * 9.0 + (ph * 9.0).sin() * 4.0;
                let a = ((1.0 - ph) * 255.0) as u8;
                p.text(c(x, 22.0 - ph * 30.0), Align2::CENTER_CENTER, if i % 2 == 0 { "♪" } else { "♫" }, FontId::proportional(18.0), Color32::from_rgba_unmultiplied(255, 214, 90, a));
            }
        }
        "video" => {
            let r = Rect::from_min_size(c(86.0, 30.0), vec2(40.0, 26.0));
            let hue = (t * 0.6) % 1.0;
            let col = Color32::from_rgb((60.0 + 120.0 * hue) as u8, (90.0 + 80.0 * (1.0 - hue)) as u8, 170);
            p.rect_filled(r, 3.0, col);
            p.rect_stroke(r, 3.0, Stroke::new(2.0_f32, steel));
            let m = r.center();
            p.add(egui::Shape::convex_polygon(vec![m + vec2(-5.0, -7.0), m + vec2(-5.0, 7.0), m + vec2(7.0, 0.0)], Color32::from_rgba_unmultiplied(255, 255, 255, 230), Stroke::NONE));
            p.rect_filled(Rect::from_min_size(c(104.0, 58.0), vec2(4.0, 6.0)), 0.0, steel);
            p.rect_filled(Rect::from_min_size(c(96.0, 63.0), vec2(20.0, 3.0)), 0.0, steel);
        }
        _ => {}
    }
}

/// Tombol pil berwarna aksen: `primary` = terisi penuh, selain itu garis tepi saja.
pub(crate) fn pill(ui: &mut egui::Ui, text: &str, accent: Color32, primary: bool) -> egui::Response {
    let label = RichText::new(text).strong().color(if primary { Color32::from_rgb(8, 16, 28) } else { accent });
    let b = egui::Button::new(label).rounding(14.0).min_size(vec2(0.0, 28.0));
    let b = if primary { b.fill(accent).stroke(Stroke::NONE) } else { b.fill(Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 24)).stroke(Stroke::new(1.2_f32, accent)) };
    ui.add(b).on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Topi di atas kepala (dibeli di toko): bentuk sederhana vektor.
pub(crate) fn draw_hat(p: &egui::Painter, kind: &str, c: egui::Pos2, k: f32) {
    let gold = Color32::from_rgb(250, 204, 21);
    match kind {
        "mahkota" => {
            let pts = vec![c + vec2(-14.0 * k, 0.0), c + vec2(-14.0 * k, -14.0 * k), c + vec2(-7.0 * k, -7.0 * k), c + vec2(0.0, -17.0 * k), c + vec2(7.0 * k, -7.0 * k), c + vec2(14.0 * k, -14.0 * k), c + vec2(14.0 * k, 0.0)];
            p.add(egui::Shape::convex_polygon(pts, gold, Stroke::new(1.2_f32, Color32::from_rgb(161, 98, 7))));
        }
        "kupluk" => {
            p.rect_filled(Rect::from_center_size(c + vec2(0.0, -7.0 * k), vec2(30.0 * k, 14.0 * k)), 7.0 * k, Color32::from_rgb(220, 60, 80));
            p.rect_filled(Rect::from_center_size(c + vec2(0.0, -1.0 * k), vec2(32.0 * k, 5.0 * k)), 2.0, Color32::from_rgb(245, 245, 245));
            p.circle_filled(c + vec2(0.0, -16.0 * k), 3.5 * k, Color32::from_rgb(245, 245, 245));
        }
        "penyihir" => {
            p.add(egui::Shape::convex_polygon(vec![c + vec2(-11.0 * k, 0.0), c + vec2(11.0 * k, 0.0), c + vec2(2.0 * k, -26.0 * k)], Color32::from_rgb(88, 60, 160), Stroke::new(1.0_f32, Color32::from_rgb(40, 24, 90))));
            p.rect_filled(Rect::from_center_size(c + vec2(0.0, 1.0 * k), vec2(34.0 * k, 4.0 * k)), 2.0, Color32::from_rgb(70, 46, 130));
            p.circle_filled(c + vec2(2.0 * k, -12.0 * k), 2.0 * k, gold);
        }
        "baling-baling" => {
            p.rect_filled(Rect::from_center_size(c + vec2(0.0, -3.0 * k), vec2(24.0 * k, 8.0 * k)), 4.0 * k, Color32::from_rgb(56, 140, 230));
            p.line_segment([c + vec2(-13.0 * k, -9.0 * k), c + vec2(13.0 * k, -9.0 * k)], Stroke::new(2.0_f32, Color32::from_rgb(230, 230, 240)));
            p.line_segment([c + vec2(0.0, -7.0 * k), c + vec2(0.0, -10.0 * k)], Stroke::new(2.0_f32, Color32::from_rgb(230, 230, 240)));
        }
        "halo" => {
            p.add(egui::Shape::ellipse_stroke(c + vec2(0.0, -8.0 * k), vec2(14.0 * k, 4.5 * k), Stroke::new(2.4_f32, gold)));
        }
        _ => {}
    }
}

/// Ikon roda (digambar dengan bentuk dasar; satu gaya garis seragam).
pub(crate) fn wheel_icon(p: &egui::Painter, i: usize, c: egui::Pos2, col: Color32) {
    let st = Stroke::new(1.8_f32, col);
    match i {
        0 => {
            // gelembung percakapan
            let b = Rect::from_center_size(c + vec2(0.0, -2.0), vec2(20.0, 14.0));
            p.rect_stroke(b, 5.0, st);
            p.line_segment([b.left_bottom() + vec2(4.0, 0.0), b.left_bottom() + vec2(3.0, 6.0)], st);
            p.line_segment([b.left_bottom() + vec2(3.0, 6.0), b.left_bottom() + vec2(9.0, 0.0)], st);
        }
        1 => {
            // koran
            let b = Rect::from_center_size(c, vec2(18.0, 20.0));
            p.rect_stroke(b, 2.0, st);
            for k in 0..3 {
                let y = b.top() + 5.0 + k as f32 * 5.0;
                p.line_segment([pos2(b.left() + 3.0, y), pos2(b.right() - 3.0, y)], Stroke::new(1.3_f32, col));
            }
        }
        2 => {
            // awan
            p.circle_stroke(c + vec2(-5.0, 2.0), 5.0, st);
            p.circle_stroke(c + vec2(2.0, -2.0), 7.0, st);
            p.circle_stroke(c + vec2(8.0, 3.0), 4.5, st);
            p.line_segment([c + vec2(-6.0, 7.5), c + vec2(9.0, 7.5)], st);
        }
        3 => {
            // bulan sabit
            p.circle_stroke(c, 9.0, st);
            p.circle_filled(c + vec2(4.5, -2.0), 8.0, Color32::from_rgb(15, 27, 48));
            p.circle_stroke(c + vec2(4.5, -2.0), 8.0, Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(15, 27, 48, 0)));
        }
        4 => {
            // grafik garis
            let pts = [c + vec2(-9.0, 6.0), c + vec2(-3.0, -1.0), c + vec2(2.0, 3.0), c + vec2(9.0, -7.0)];
            p.add(egui::Shape::line(pts.to_vec(), st));
            p.line_segment([c + vec2(-10.0, 9.0), c + vec2(10.0, 9.0)], Stroke::new(1.2_f32, col));
        }
        5 => {
            // z z (tidur)
            p.text(c + vec2(-3.0, 2.0), Align2::CENTER_CENTER, "z", FontId::proportional(16.0), col);
            p.text(c + vec2(5.0, -6.0), Align2::CENTER_CENTER, "z", FontId::proportional(11.0), col);
        }
        6 => {
            // buku catatan
            let b = Rect::from_center_size(c, vec2(16.0, 20.0));
            p.rect_stroke(b, 2.0, st);
            for k in 0..3 {
                let y = b.top() + 6.0 + k as f32 * 5.0;
                p.line_segment([pos2(b.left() + 4.0, y), pos2(b.right() - 3.0, y)], Stroke::new(1.2_f32, col));
            }
            p.line_segment([pos2(b.left() + 2.5, b.top() - 2.0), pos2(b.left() + 2.5, b.top() + 3.0)], st);
        }
        _ => {
            // cakar / sahabat: bantalan + 3 jari
            p.circle_filled(c + vec2(0.0, 4.0), 6.0, col);
            for (dx, dy) in [(-8.0, -3.0), (-3.0, -8.0), (3.0, -8.0), (8.0, -3.0)] {
                p.circle_filled(c + vec2(dx, dy), 2.8, col);
            }
        }
    }
}

pub(crate) fn draw(p: &egui::Painter, rows: &[String], main: (u8, u8, u8), origin: egui::Pos2, flip: bool, scale: f32) {
    for (y, row) in rows.iter().enumerate() {
        for (x, ch) in row.bytes().enumerate() {
            if let Some((r, g, b)) = pixel_rgb(ch, main) {
                let cx = if flip { 15 - x } else { x } as f32;
                let a = origin + vec2(cx * scale, y as f32 * scale);
                p.rect_filled(Rect::from_min_size(a, vec2(scale + 0.5, scale + 0.5)), 0.0, Color32::from_rgb(r, g, b));
            }
        }
    }
}

/// Ekspresi peliharaan sebagai respons suasana hati Bos (alis, keringat, hati, air mata, bintang).
pub(crate) fn expression(p: &egui::Painter, emo: &str, o: egui::Pos2, t: f64) {
    let px = |x: f32, y: f32| o + vec2(x * SCALE, y * SCALE);
    let line = |a: egui::Pos2, b: egui::Pos2, c: Color32| p.line_segment([a, b], Stroke::new(2.0_f32, c));
    let ink = Color32::from_rgb(14, 20, 30);
    match emo {
        // Khawatir: alis miring ke atas-tengah + tetes keringat
        "stres" | "marah" => {
            line(px(3.6, 4.6), px(6.4, 4.0), ink);
            line(px(9.6, 4.0), px(12.4, 4.6), ink);
            let bob = ((t * 3.0).sin() as f32) * 2.0;
            p.circle_filled(px(14.2, 4.5) + vec2(0.0, bob), 3.0, Color32::from_rgb(120, 200, 255));
        }
        // Peduli: alis sedih + air mata + hati kecil
        "menangis" => {
            line(px(3.4, 4.8), px(6.4, 4.0), ink);
            line(px(9.6, 4.0), px(12.6, 4.8), ink);
            for k in 0..2 {
                let ph = ((t * 1.6 + k as f64 * 0.5) % 1.0) as f32;
                let x = if k == 0 { 5.0 } else { 11.0 };
                p.circle_filled(px(x, 8.0) + vec2(0.0, ph * 11.0), 2.3 * (1.0 - ph * 0.4), Color32::from_rgba_unmultiplied(120, 200, 255, (255.0 * (1.0 - ph * 0.7)) as u8));
            }
            line(px(6.0, 12.4), px(7.5, 11.4), ink);
            line(px(7.5, 11.4), px(9.0, 12.4), ink);
        }
        "sedih" => {
            line(px(3.6, 4.0), px(6.4, 4.6), ink);
            line(px(9.6, 4.6), px(12.4, 4.0), ink);
            p.circle_filled(px(5.0, 8.4) + vec2(0.0, ((t * 2.0) % 1.0) as f32 * 8.0), 2.5, Color32::from_rgb(120, 200, 255));
            p.text(px(14.0, 1.0), Align2::CENTER_CENTER, "♥", FontId::proportional(16.0), Color32::from_rgb(255, 130, 170));
        }
        // Mengantuk: kelopak turun + titik-titik
        "lelah" => {
            line(px(3.8, 5.8), px(6.2, 5.8), ink);
            line(px(9.8, 5.8), px(12.2, 5.8), ink);
            p.text(px(14.0, 2.0), Align2::CENTER_CENTER, "…", FontId::proportional(18.0), Color32::from_rgb(190, 205, 235));
        }
        // Ikut senang: bintang berkedip + hati
        "gembira" => {
            let on = (t * 4.0) as u64 % 2 == 0;
            let c = if on { Color32::from_rgb(255, 224, 120) } else { Color32::from_rgb(255, 170, 60) };
            p.text(px(0.5, 1.5), Align2::CENTER_CENTER, "✦", FontId::proportional(16.0), c);
            p.text(px(15.0, 0.5), Align2::CENTER_CENTER, "✦", FontId::proportional(14.0), c);
            p.text(px(14.5, 6.0), Align2::CENTER_CENTER, "♥", FontId::proportional(14.0), Color32::from_rgb(255, 130, 170));
        }
        // Helm pengaman: peringatan git (branch utama / belum commit)
        "helm" => {
            let hat = Color32::from_rgb(255, 190, 40);
            p.rect_filled(Rect::from_min_size(px(3.0, 0.2), vec2(10.0 * SCALE, 2.2 * SCALE)), 4.0, hat);
            p.rect_filled(Rect::from_min_size(px(2.0, 2.0), vec2(12.0 * SCALE, 0.9 * SCALE)), 1.0, Color32::from_rgb(230, 150, 20));
            p.text(px(14.5, 1.0), Align2::CENTER_CENTER, "!", FontId::proportional(16.0), Color32::from_rgb(255, 90, 100));
        }
        // Pusing: error di log/sintaks -> mata berputar + kedip merah
        "pusing" => {
            let a = (t * 6.0) as f32;
            for k in 0..3 {
                let r = 7.0 + k as f32 * 5.0;
                p.circle_filled(px(8.0, 1.0) + vec2((a + k as f32 * 2.1).cos() * r, (a + k as f32 * 2.1).sin() * r * 0.35), 2.0, Color32::from_rgb(255, 214, 90));
            }
            if (t * 3.0) as u64 % 2 == 0 {
                p.rect_filled(Rect::from_min_size(o, vec2(16.0 * SCALE, 16.0 * SCALE)), 0.0, Color32::from_rgba_unmultiplied(255, 40, 60, 60));
            }
            line(px(3.8, 6.2), px(6.2, 7.6), ink);
            line(px(3.8, 7.6), px(6.2, 6.2), ink);
            line(px(9.8, 6.2), px(12.2, 7.6), ink);
            line(px(9.8, 7.6), px(12.2, 6.2), ink);
        }
        _ => {}
    }
}


/// Animasi aksi: properti piksel di sisi kanan peliharaan sesuai yang sedang dikerjakan.
/// `o` = pojok kiri-atas sprite (80x80 px), `age` = detik sejak aksi mulai, `t` = waktu global.
pub(crate) fn draw_act(p: &egui::Painter, kind: &str, text: &str, o: egui::Pos2, age: f32, t: f32, main: (u8, u8, u8)) {
    let c = |x: f32, y: f32| o + vec2(x, y);
    let body = Color32::from_rgb(main.0, main.1, main.2);
    let white = Color32::from_rgb(235, 245, 255);
    let steel = Color32::from_rgb(86, 94, 112);
    let line = |a: egui::Pos2, b: egui::Pos2, w: f32, col: Color32| p.line_segment([a, b], Stroke::new(w, col));
    match kind {
        "tembak" => {
            // 0-0.3 angkat pistol, 0.3 tembak, 0.3-0.55 peluru melaju, 0.55+ iklan hancur berkeping
            let recoil = if (0.3..0.45).contains(&age) { -3.0 } else { 0.0 };
            let (gx, gy) = (66.0, 46.0 + recoil);
            line(c(60.0, 52.0), c(gx, gy + 5.0), 5.0, body); // lengan
            p.rect_filled(Rect::from_min_size(c(gx, gy), vec2(24.0, 8.0)), 1.0, steel); // slide
            p.rect_filled(Rect::from_min_size(c(gx + 22.0, gy + 2.0), vec2(4.0, 4.0)), 0.0, Color32::from_rgb(40, 44, 56)); // moncong
            p.rect_filled(Rect::from_min_size(c(gx, gy + 8.0), vec2(8.0, 13.0)), 1.0, Color32::from_rgb(120, 72, 40)); // gagang
            p.rect_stroke(Rect::from_min_size(c(gx + 8.0, gy + 8.0), vec2(7.0, 6.0)), 1.0, Stroke::new(1.5_f32, steel)); // pelatuk
            let muzzle = c(gx + 28.0, gy + 4.0);
            let target = c(118.0, 38.0);
            if (0.3..0.45).contains(&age) {
                for k in 0..8 {
                    let a = k as f32 * 0.785;
                    line(muzzle, muzzle + vec2(a.cos() * 11.0, a.sin() * 11.0), 2.0, Color32::from_rgb(255, 214, 90));
                }
                p.circle_filled(muzzle, 5.0, Color32::from_rgb(255, 250, 200));
                p.text(muzzle + vec2(-2.0, -16.0), Align2::CENTER_CENTER, "PEW!", FontId::proportional(12.0), Color32::from_rgb(255, 214, 90));
            }
            if age < 0.55 {
                // iklan utuh
                let wob = ((t * 14.0).sin()) * 1.0;
                let r = Rect::from_min_size(target + vec2(wob, 0.0), vec2(26.0, 20.0));
                p.rect_filled(r, 2.0, Color32::from_rgb(200, 50, 60));
                p.rect_stroke(r, 2.0, Stroke::new(1.5_f32, Color32::WHITE));
                p.text(r.center(), Align2::CENTER_CENTER, "AD", FontId::proportional(12.0), Color32::WHITE);
                if age >= 0.3 {
                    let k = ((age - 0.3) / 0.25).clamp(0.0, 1.0);
                    let bx = muzzle.x + (target.x - muzzle.x) * k;
                    line(muzzle, egui::pos2(bx, muzzle.y), 1.5, Color32::from_rgba_unmultiplied(255, 230, 150, 160));
                    p.circle_filled(egui::pos2(bx, muzzle.y), 2.5, Color32::from_rgb(255, 240, 180));
                }
            } else {
                // pecahan melayang + teks domain
                let k = ((age - 0.55) / 0.9).clamp(0.0, 1.0);
                for i in 0..9 {
                    let a = i as f32 * 0.7 + 0.3;
                    let d = 6.0 + k * 34.0;
                    let pc = target + vec2(13.0, 10.0) + vec2(a.cos() * d, a.sin() * d * 0.8 + k * k * 14.0);
                    let sz = 6.0 * (1.0 - k) + 1.5;
                    p.rect_filled(Rect::from_center_size(pc, vec2(sz, sz)), 0.0, Color32::from_rgba_unmultiplied(220, 60, 70, (255.0 * (1.0 - k)) as u8));
                }
                if !text.is_empty() {
                    let t: String = text.chars().take(20).collect();
                    p.text(c(40.0, 96.0), Align2::CENTER_CENTER, format!("✖ {t}"), FontId::proportional(10.0), Color32::from_rgba_unmultiplied(255, 190, 190, (255.0 * (1.0 - k * 0.6)) as u8));
                }
            }
        }
        "cari" => {
            let (cx, cy) = (104.0 + (t * 3.0).sin() * 10.0, 38.0 + (t * 2.0).cos() * 8.0);
            p.circle_stroke(c(cx, cy), 10.0, Stroke::new(3.0_f32, white));
            line(c(cx + 7.0, cy + 7.0), c(cx + 17.0, cy + 17.0), 4.0, white);
            p.circle_filled(c(cx - 3.0, cy - 3.0), 2.0, Color32::from_rgba_unmultiplied(255, 255, 255, 120));
        }
        "sapu" => {
            let ang = (t * 7.0).sin() * 0.45;
            let top = c(112.0, 26.0);
            let bot = top + vec2((0.6 + ang).sin() * 40.0, (0.6 + ang).cos() * 40.0);
            line(top, bot, 3.0, Color32::from_rgb(160, 110, 60));
            p.rect_filled(Rect::from_center_size(bot, vec2(16.0, 9.0)), 2.0, Color32::from_rgb(220, 180, 80));
            for k in 0..4 {
                let sp = bot + vec2(-12.0 - k as f32 * 7.0 + ((t * 5.0 + k as f32).sin()) * 3.0, 6.0 - k as f32 * 3.0);
                p.circle_filled(sp, 1.8, Color32::from_rgba_unmultiplied(255, 240, 160, (230 - k * 50) as u8));
            }
        }
        "gemini" => {
            // bintang percikan berputar + titik mengorbit (warna biru-ungu khas Gemini)
            let cen = c(112.0, 42.0);
            for (i, (dx, dy, sz)) in [(0.0f32, 0.0f32, 13.0f32), (22.0, -14.0, 7.0), (-6.0, 22.0, 6.0)].iter().enumerate() {
                let pulse = 0.7 + 0.3 * (t * 4.0 + i as f32 * 1.7).sin();
                let ctr = cen + vec2(*dx, *dy);
                let s = *sz * pulse;
                let col = Color32::from_rgb(120 + (i as u8) * 30, 140, 255);
                p.add(egui::Shape::convex_polygon(vec![ctr + vec2(0.0, -s), ctr + vec2(s * 0.28, -s * 0.28), ctr + vec2(s, 0.0), ctr + vec2(s * 0.28, s * 0.28), ctr + vec2(0.0, s), ctr + vec2(-s * 0.28, s * 0.28), ctr + vec2(-s, 0.0), ctr + vec2(-s * 0.28, -s * 0.28)], col, Stroke::NONE));
            }
            for k in 0..3 {
                let a = t * 3.0 + k as f32 * 2.09;
                p.circle_filled(cen + vec2(a.cos() * 26.0, a.sin() * 18.0), 2.2, Color32::from_rgb(190, 150, 255));
            }
        }
        "web" => {
            let cen = c(112.0, 46.0);
            p.circle_filled(cen, 17.0, Color32::from_rgb(30, 80, 150));
            p.circle_stroke(cen, 17.0, Stroke::new(2.0_f32, Color32::from_rgb(150, 210, 255)));
            line(cen + vec2(-17.0, 0.0), cen + vec2(17.0, 0.0), 1.2, Color32::from_rgb(150, 210, 255));
            let rx = 17.0 * (t * 1.6).cos().abs();
            let mut prev = cen + vec2(0.0, -17.0);
            for i in 1..=24 {
                let a = std::f32::consts::TAU * i as f32 / 24.0;
                let pt = cen + vec2(a.sin() * rx, -a.cos() * 17.0);
                line(prev, pt, 1.0, Color32::from_rgb(150, 210, 255));
                prev = pt;
            }
            let a = t * 4.0;
            p.circle_filled(cen + vec2(a.cos() * 24.0, a.sin() * 10.0 - 6.0), 2.5, Color32::from_rgb(255, 214, 90));
            line(cen + vec2(14.0, 14.0), cen + vec2(26.0, 26.0), 3.0, Color32::from_rgb(200, 200, 210));
        }
        "kecerahan" => {
            let cen = c(112.0, 44.0);
            let pulse = 1.0 + 0.15 * (t * 4.0).sin();
            p.circle_filled(cen, 10.0 * pulse, Color32::from_rgb(255, 214, 90));
            for k in 0..8 {
                let a = k as f32 * 0.785 + t;
                line(cen + vec2(a.cos() * 15.0, a.sin() * 15.0), cen + vec2(a.cos() * (21.0 + 3.0 * pulse), a.sin() * (21.0 + 3.0 * pulse)), 2.5, Color32::from_rgb(255, 230, 140));
            }
        }
        "volume" => {
            let o2 = c(98.0, 44.0);
            p.add(egui::Shape::convex_polygon(vec![o2 + vec2(0.0, -6.0), o2 + vec2(8.0, -6.0), o2 + vec2(18.0, -14.0), o2 + vec2(18.0, 14.0), o2 + vec2(8.0, 6.0), o2 + vec2(0.0, 6.0)], Color32::from_rgb(220, 230, 245), Stroke::NONE));
            for k in 0..3 {
                let ph = (t * 1.8 + k as f32 * 0.33) % 1.0;
                let r = 8.0 + ph * 18.0;
                let col = Color32::from_rgba_unmultiplied(150, 210, 255, ((1.0 - ph) * 255.0) as u8);
                let mut prev = o2 + vec2(20.0 + r * (-0.6f32).cos(), r * (-0.6f32).sin());
                for i in 1..=8 {
                    let a = -0.6 + 1.2 * i as f32 / 8.0;
                    let pt = o2 + vec2(20.0 + r * a.cos() - 8.0, r * a.sin());
                    line(prev, pt, 2.0, col);
                    prev = pt;
                }
            }
        }
        "wifi" => {
            let base = c(112.0, 62.0);
            let lit = ((t * 2.5) as usize) % 4;
            for k in 0..3 {
                let r = 10.0 + k as f32 * 9.0;
                let col = if lit > k { Color32::from_rgb(110, 230, 150) } else { Color32::from_rgb(70, 90, 100) };
                let mut prev = base + vec2((-0.9f32).sin() * r, -(0.9f32).cos() * r);
                for i in 1..=8 {
                    let a = -0.9 + 1.8 * i as f32 / 8.0;
                    let pt = base + vec2(a.sin() * r, -a.cos() * r);
                    line(prev, pt, 3.0, col);
                    prev = pt;
                }
            }
            p.circle_filled(base, 3.0, if lit > 0 { Color32::from_rgb(110, 230, 150) } else { Color32::from_rgb(70, 90, 100) });
        }
        "terminal" => {
            let r = Rect::from_min_size(c(92.0, 34.0), vec2(48.0, 30.0));
            p.rect_filled(r, 3.0, Color32::from_rgb(10, 14, 22));
            p.rect_stroke(r, 3.0, Stroke::new(1.5_f32, Color32::from_rgb(110, 120, 140)));
            let n = ((t * 8.0) as usize) % 9;
            p.text(r.min + vec2(5.0, 4.0), Align2::LEFT_TOP, format!("> {}", "▪".repeat(n)), FontId::monospace(10.0), Color32::from_rgb(110, 230, 150));
            if (t * 3.0) as i32 % 2 == 0 {
                p.rect_filled(Rect::from_min_size(r.min + vec2(8.0 + n as f32 * 4.5 + 6.0, 18.0), vec2(5.0, 8.0)), 0.0, Color32::from_rgb(110, 230, 150));
            }
        }
        "buka" => {
            let k = (age * 1.5).min(1.0);
            let r = Rect::from_min_size(c(98.0, 56.0 - 22.0 * k), vec2(30.0, 38.0));
            p.rect_filled(r, 2.0, Color32::from_rgb(244, 242, 232));
            p.rect_filled(Rect::from_min_size(r.min + vec2(22.0, 0.0), vec2(8.0, 8.0)), 0.0, Color32::from_rgb(200, 196, 180));
            for i in 0..4 {
                line(r.min + vec2(5.0, 12.0 + i as f32 * 6.0), r.min + vec2(25.0, 12.0 + i as f32 * 6.0), 1.2, Color32::from_rgb(140, 140, 150));
            }
            p.rect_filled(Rect::from_min_size(c(92.0, 66.0), vec2(42.0, 16.0)), 2.0, Color32::from_rgb(240, 180, 70));
        }
        "pikir" => {
            for k in 0..3 {
                let r = 3.5 + ((t * 4.0 + k as f32 * 1.4).sin() * 0.5 + 0.5) * 3.0;
                p.circle_filled(c(48.0 + k as f32 * 14.0, -10.0), r, white);
            }
        }
        "baca" => {
            let flip = ((t * 3.0) % 1.0) as f32;
            p.rect_filled(Rect::from_min_size(c(92.0, 40.0), vec2(18.0, 22.0)), 1.0, Color32::from_rgb(240, 236, 220));
            p.rect_filled(Rect::from_min_size(c(110.0, 40.0), vec2(18.0, 22.0)), 1.0, Color32::from_rgb(226, 222, 204));
            for i in 0..4 {
                line(c(95.0, 45.0 + i as f32 * 5.0), c(107.0, 45.0 + i as f32 * 5.0), 1.0, Color32::from_rgb(120, 120, 130));
                line(c(113.0, 45.0 + i as f32 * 5.0), c(125.0, 45.0 + i as f32 * 5.0), 1.0, Color32::from_rgb(120, 120, 130));
            }
            line(c(110.0, 38.0), c(110.0 + (flip - 0.5) * 22.0, 62.0), 2.0, Color32::from_rgb(255, 255, 255));
        }
        "email" => {
            let y = 62.0 - (age * 24.0) % 40.0;
            let r = Rect::from_min_size(c(100.0, y), vec2(24.0, 16.0));
            p.rect_filled(r, 2.0, white);
            p.rect_stroke(r, 2.0, Stroke::new(1.5_f32, Color32::from_rgb(90, 130, 200)));
            line(r.left_top(), r.center(), 1.5, Color32::from_rgb(90, 130, 200));
            line(r.right_top(), r.center(), 1.5, Color32::from_rgb(90, 130, 200));
        }
        "koran" => {
            let r = Rect::from_min_size(c(98.0, 36.0), vec2(26.0, 32.0));
            p.rect_filled(r, 1.0, Color32::from_rgb(232, 228, 212));
            p.rect_filled(Rect::from_min_size(r.min + vec2(3.0, 3.0), vec2(20.0, 6.0)), 0.0, Color32::from_rgb(60, 60, 70));
            for i in 0..4 {
                line(r.min + vec2(3.0, 14.0 + i as f32 * 4.5), r.min + vec2(23.0, 14.0 + i as f32 * 4.5), 1.0, Color32::from_rgb(110, 110, 120));
            }
        }
        "alarm" => {
            let sh = (t * 22.0).sin() * 2.5;
            p.circle_filled(c(108.0 + sh, 44.0), 11.0, Color32::from_rgb(255, 200, 60));
            p.rect_filled(Rect::from_min_size(c(96.0 + sh, 50.0), vec2(24.0, 5.0)), 1.0, Color32::from_rgb(230, 160, 30));
            p.circle_filled(c(108.0 + sh, 60.0), 3.0, Color32::from_rgb(120, 80, 20));
            line(c(92.0, 32.0), c(86.0, 28.0), 2.0, Color32::from_rgb(255, 214, 90));
            line(c(124.0, 32.0), c(130.0, 28.0), 2.0, Color32::from_rgb(255, 214, 90));
        }
        "ketik" => {
            let r = Rect::from_min_size(c(88.0, 58.0), vec2(40.0, 14.0));
            p.rect_filled(r, 2.0, steel);
            for i in 0..8 {
                let on = ((t * 9.0 + i as f32 * 1.7).sin() > 0.3) as u8;
                p.rect_filled(Rect::from_min_size(r.min + vec2(3.0 + i as f32 * 4.5, 3.0 + (i % 2) as f32 * 5.0), vec2(3.0, 3.0)), 0.0, if on == 1 { white } else { Color32::from_rgb(150, 160, 180) });
            }
        }
        "baterai" => {
            let lvl = ((t * 1.5).sin() * 0.5 + 0.5).clamp(0.1, 1.0);
            let r = Rect::from_min_size(c(94.0, 44.0), vec2(32.0, 16.0));
            p.rect_stroke(r, 2.0, Stroke::new(2.0_f32, white));
            p.rect_filled(Rect::from_min_size(r.max + vec2(0.0, -11.0), vec2(3.0, 6.0)), 0.0, white);
            p.rect_filled(Rect::from_min_size(r.min + vec2(2.0, 2.0), vec2(28.0 * lvl, 12.0)), 1.0, if lvl < 0.3 { Color32::from_rgb(255, 90, 100) } else { Color32::from_rgb(110, 230, 140) });
        }
        "palu" => {
            let ang = -0.2 + ((t * 8.0).sin() * 0.5 + 0.5) * 1.1;
            let pivot = c(100.0, 62.0);
            let tip = pivot + vec2(ang.sin() * 28.0, -ang.cos() * 28.0);
            line(pivot, tip, 4.0, Color32::from_rgb(160, 110, 60));
            p.rect_filled(Rect::from_center_size(tip, vec2(14.0, 9.0)), 1.0, steel);
            if ang > 0.8 {
                for k in 0..5 {
                    let a = k as f32 * 1.2;
                    p.circle_filled(c(116.0 + a.cos() * 8.0, 62.0 + a.sin() * 5.0), 1.6, Color32::from_rgb(255, 214, 90));
                }
            }
        }
        "cek" => {
            let r = Rect::from_min_size(c(92.0, 40.0), vec2(40.0, 24.0));
            p.rect_filled(r, 2.0, Color32::from_rgb(10, 30, 24));
            let mut prev = r.min + vec2(0.0, 12.0);
            for i in 1..=20 {
                let x = i as f32 * 2.0;
                let ph = (x * 0.35 - t * 6.0).sin();
                let y = 12.0 - if ph > 0.85 { 9.0 } else if ph < -0.85 { -6.0 } else { 0.0 };
                let pt = r.min + vec2(x, y);
                line(prev, pt, 1.5, Color32::from_rgb(90, 255, 160));
                prev = pt;
            }
        }
        "graf" => {
            let pts = [(100.0, 36.0), (122.0, 30.0), (114.0, 54.0), (96.0, 62.0), (130.0, 60.0)];
            let n = ((age * 4.0) as usize + 1).min(pts.len());
            for i in 0..n {
                for j in 0..i {
                    if (i + j) % 2 == 1 || i == j + 1 {
                        line(c(pts[i].0, pts[i].1), c(pts[j].0, pts[j].1), 1.5, Color32::from_rgba_unmultiplied(120, 190, 255, 180));
                    }
                }
            }
            for (i, pt) in pts.iter().take(n).enumerate() {
                p.circle_filled(c(pt.0, pt.1), 4.0 + ((t * 3.0 + i as f32).sin() * 0.5 + 0.5) * 2.0, if i % 2 == 0 { Color32::from_rgb(110, 200, 255) } else { Color32::from_rgb(255, 214, 90) });
            }
        }
        "tawa" => {
            let w = (t * 10.0).sin() * 3.0;
            p.text(c(110.0 + w, 28.0), Align2::CENTER_CENTER, "XD", FontId::proportional(20.0), Color32::from_rgb(255, 214, 90));
            p.text(c(92.0 + w, 14.0), Align2::CENTER_CENTER, "haha", FontId::proportional(10.0), white);
        }
        _ => {}
    }
}

pub(crate) fn px_at(o: egui::Pos2, x: f32, y: f32) -> egui::Pos2 {
    o + vec2(x * SCALE, y * SCALE)
}

pub(crate) fn agent_color(i: usize) -> Color32 {
    let c = Animal::from_idx(i).rgb();
    Color32::from_rgb(c.0, c.1, c.2)
}

/// Selisih horizontal (piksel) agar peliharaan menoleh ke kursor: -3..3.
pub(crate) fn gaze_dx(ctx: &egui::Context, w: f32) -> f32 {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::Foundation::POINT;
        use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
        let mut p = POINT { x: 0, y: 0 };
        if GetCursorPos(&mut p) == 0 {
            return 0.0;
        }
        let ppp = ctx.pixels_per_point();
        if let Some(r) = ctx.input(|i| i.viewport().outer_rect) {
            let cx = (r.min.x + w / 2.0) * ppp;
            return (((p.x as f32 - cx) / (400.0 * ppp)).clamp(-1.0, 1.0) * 3.0).round();
        }
    }
    let _ = (ctx, w);
    0.0
}

/// Gaya global: sudut membulat, jarak lega, tombol biru-tua dengan sorotan saat disentuh.
pub(crate) fn apply_style(ctx: &egui::Context) {
    let mut st = (*ctx.style()).clone();
    st.spacing.button_padding = vec2(12.0, 5.0);
    st.spacing.item_spacing = vec2(8.0, 6.0);
    st.spacing.interact_size.y = 26.0;
    // Selaras dengan dasbor/obrolan web: sudut hampir persegi, aksen kuning-oranye, latar biru-abu.
    let r = egui::Rounding::same(4.0);
    let v = &mut st.visuals;
    v.widgets.inactive.rounding = r;
    v.widgets.hovered.rounding = r;
    v.widgets.active.rounding = r;
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(27, 33, 44);
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(39, 48, 64));
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(36, 43, 58);
    v.widgets.hovered.bg_stroke = Stroke::new(1.2_f32, Color32::from_rgb(232, 163, 61));
    v.widgets.active.weak_bg_fill = Color32::from_rgb(70, 52, 24);
    v.selection.bg_fill = Color32::from_rgb(120, 84, 30);
    v.extreme_bg_color = Color32::from_rgb(16, 19, 26);
    ctx.set_style(st);
}

