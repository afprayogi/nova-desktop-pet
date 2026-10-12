//! Pengekstrak teks PDF yang TOLERAN: PDF buku (mis. Cengage) sering punya peta ToUnicode yang tidak lolos parser ketat lopdf/pdf-extract,
//! sehingga seluruh halaman gagal. Di sini peta karakter dibaca secara longgar (bfchar/bfrange), isi halaman diurai langsung dari operator
//! teks (Tf, Tj, TJ, Td, T*, '...'), dan font tanpa peta jatuh ke Latin-1. Halaman yang gagal dilewati, tidak membatalkan seluruh buku.
use lopdf::{Document, Object};
use std::collections::HashMap;

fn hex(s: &str) -> Option<u32> {
    u32::from_str_radix(s, 16).ok()
}

fn utf16(s: &str) -> String {
    let u: Vec<u16> = (0..s.len() / 4).filter_map(|i| u16::from_str_radix(&s[i * 4..i * 4 + 4], 16).ok()).collect();
    String::from_utf16_lossy(&u)
}

/// Peta kode → teks dari aliran CMap (bfchar dan bfrange), tanpa gagal pada sintaks aneh.
pub fn urai_cmap(b: &[u8]) -> HashMap<u32, String> {
    let t = String::from_utf8_lossy(b);
    let tok = regex::Regex::new(r"<([0-9A-Fa-f]*)>|\[|\]").unwrap();
    let mut m = HashMap::new();
    for (awal, akhir, jenis) in [("beginbfchar", "endbfchar", 1), ("beginbfrange", "endbfrange", 2)] {
        let mut dari = 0;
        while let Some(i) = t[dari..].find(awal) {
            let mulai = dari + i + awal.len();
            let Some(j) = t[mulai..].find(akhir) else { break };
            let blok = &t[mulai..mulai + j];
            dari = mulai + j;
            let toks: Vec<(&str, Option<&str>)> = tok.captures_iter(blok).map(|c| (c.get(0).unwrap().as_str(), c.get(1).map(|x| x.as_str()))).collect();
            let mut k = 0;
            if jenis == 1 {
                while k + 1 < toks.len() {
                    if let (Some(a), Some(d)) = (toks[k].1.and_then(hex), toks[k + 1].1) {
                        m.insert(a, utf16(d));
                    }
                    k += 2;
                }
            } else {
                while k + 2 < toks.len() {
                    let (Some(lo), Some(hi)) = (toks[k].1.and_then(hex), toks[k + 1].1.and_then(hex)) else { k += 1; continue };
                    if toks[k + 2].0 == "[" {
                        let mut n = 0;
                        k += 3;
                        while k < toks.len() && toks[k].0 != "]" {
                            if let Some(d) = toks[k].1 {
                                m.insert(lo + n, utf16(d));
                                n += 1;
                            }
                            k += 1;
                        }
                        k += 1;
                    } else {
                        if let Some(d) = toks[k + 2].1 {
                            let basis = utf16(d);
                            let c0 = basis.chars().last().map(|c| c as u32).unwrap_or(0);
                            let awal_s: String = basis.chars().take(basis.chars().count().saturating_sub(1)).collect();
                            for (n, kode) in (lo..=hi.min(lo + 4096)).enumerate() {
                                m.insert(kode, format!("{awal_s}{}", char::from_u32(c0 + n as u32).unwrap_or('?')));
                            }
                        }
                        k += 3;
                    }
                }
            }
        }
    }
    m
}

struct Dek {
    dua_byte: bool,
    peta: HashMap<u32, String>,
}

impl Dek {
    fn teks(&self, b: &[u8], out: &mut String) {
        if self.dua_byte {
            for c in b.chunks(2) {
                let k = if c.len() == 2 { (c[0] as u32) << 8 | c[1] as u32 } else { c[0] as u32 };
                match self.peta.get(&k) {
                    Some(s) => out.push_str(s),
                    None if (32..127).contains(&k) => out.push(k as u8 as char),
                    None => {}
                }
            }
        } else {
            for &x in b {
                match self.peta.get(&(x as u32)) {
                    Some(s) => out.push_str(s),
                    None if x >= 32 => out.push(x as char), // Latin-1
                    None => {}
                }
            }
        }
    }
}

fn dekoder(doc: &Document, font: &lopdf::Dictionary) -> Dek {
    let dua_byte = font.get(b"Subtype").ok().and_then(|o| o.as_name().ok()).is_some_and(|n| n == b"Type0");
    let peta = font
        .get(b"ToUnicode")
        .ok()
        .and_then(|o| o.as_reference().ok())
        .and_then(|id| doc.get_object(id).ok())
        .and_then(|o| o.as_stream().ok())
        .and_then(|s| s.decompressed_content().ok().or_else(|| Some(s.content.clone())))
        .map(|b| urai_cmap(&b))
        .unwrap_or_default();
    Dek { dua_byte, peta }
}

/// Teks satu halaman (kosong bila tidak terbaca).
pub fn halaman(doc: &Document, nomor: u32) -> String {
    let Some(&pid) = doc.get_pages().get(&nomor) else { return String::new() };
    let fonts: HashMap<Vec<u8>, Dek> = doc.get_page_fonts(pid).map(|f| f.into_iter().map(|(n, d)| (n, dekoder(doc, d))).collect()).unwrap_or_default();
    let Ok(isi) = doc.get_and_decode_page_content(pid) else { return String::new() };
    let (mut out, mut aktif): (String, Option<&Dek>) = (String::new(), None);
    for op in &isi.operations {
        match op.operator.as_str() {
            "Tf" => aktif = op.operands.first().and_then(|o| o.as_name().ok()).and_then(|n| fonts.get(n)),
            "Tj" | "'" | "\"" => {
                if matches!(op.operator.as_str(), "'" | "\"") {
                    out.push('\n');
                }
                if let (Some(d), Some(Object::String(b, _))) = (aktif, op.operands.last()) {
                    d.teks(b, &mut out);
                }
            }
            "TJ" => {
                if let (Some(d), Some(Object::Array(a))) = (aktif, op.operands.first()) {
                    for x in a {
                        match x {
                            Object::String(b, _) => d.teks(b, &mut out),
                            n => {
                                if n.as_float().is_ok_and(|v| v < -200.0) && !out.ends_with(' ') {
                                    out.push(' ');
                                }
                            }
                        }
                    }
                }
            }
            "Td" | "TD" => {
                let dy = op.operands.get(1).and_then(|o| o.as_float().ok()).unwrap_or(0.0);
                out.push(if dy.abs() > 1.0 { '\n' } else { ' ' });
            }
            "T*" | "ET" => out.push('\n'),
            "Tm" => out.push(' '),
            _ => {}
        }
    }
    out.split('\n').map(|l| l.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n")
}

/// Seluruh dokumen, halaman demi halaman; berhenti setelah `max` karakter.
pub fn dokumen(path: &str, max: usize) -> Result<String, String> {
    let doc = Document::load(path).map_err(|e| e.to_string())?;
    let mut out = String::new();
    for n in doc.get_pages().keys().copied().collect::<Vec<u32>>() {
        if out.chars().count() >= max {
            break;
        }
        if let Ok(t) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| halaman(&doc, n))) {
            if !t.is_empty() {
                out.push_str(&t);
                out.push_str("\n\n");
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lenient_cmap() {
        let c = b"/CIDInit begincmap 1 begincodespacerange <00> <FF> endcodespacerange 2 beginbfchar <41> <0058> <42> <00590059> endbfchar 2 beginbfrange <61> <63> <0061> <70> <71> [<0041> <0042>] endbfrange garbage ((( endcmap";
        let m = urai_cmap(c);
        assert_eq!(m[&0x41], "X");
        assert_eq!(m[&0x42], "YY");
        assert_eq!(m[&0x62], "b");
        assert_eq!(m[&0x71], "B");
    }
    #[test]
    fn decoder_falls_back_to_latin1() {
        let d = Dek { dua_byte: false, peta: HashMap::new() };
        let mut s = String::new();
        d.teks(b"Motor 3", &mut s);
        assert_eq!(s, "Motor 3");
    }
}
