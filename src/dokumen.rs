//! Penulis dokumen tanpa dependensi baru: DOCX (OOXML minimal lewat `zip`) dan PDF sederhana (lewat `lopdf`, font Helvetica).
//! Dipakai untuk ekspor laporan. PDF hanya memuat huruf Latin; simbol di luar itu diganti padanan ASCII.
use lopdf::{dictionary, Document, Object, Stream};
use std::io::Write;
use std::path::Path;

fn xml(s: &str) -> String {
    s.chars().filter(|c| !c.is_control() || *c == '\t').map(|c| match c {
        '&' => "&amp;".to_string(),
        '<' => "&lt;".to_string(),
        '>' => "&gt;".to_string(),
        c => c.to_string(),
    }).collect()
}

/// Judul + baris teks → berkas .docx yang bisa dibuka Word/LibreOffice.
pub fn tulis_docx(path: &Path, judul: &str, baris: &[String]) -> Result<(), String> {
    let mut isi = format!("<w:p><w:pPr><w:pStyle w:val=\"Title\"/></w:pPr><w:r><w:rPr><w:b/><w:sz w:val=\"36\"/></w:rPr><w:t>{}</w:t></w:r></w:p>", xml(judul));
    for b in baris {
        let tebal = b.starts_with('#');
        let t = b.trim_start_matches('#').trim();
        isi.push_str(&format!("<w:p><w:r>{}<w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>", if tebal { "<w:rPr><w:b/><w:sz w:val=\"28\"/></w:rPr>" } else { "" }, xml(t)));
    }
    let dokumen = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>{isi}<w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/><w:pgMar w:top=\"1134\" w:right=\"1134\" w:bottom=\"1134\" w:left=\"1134\"/></w:sectPr></w:body></w:document>");
    let f = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut z = zip::ZipWriter::new(f);
    let o = zip::write::SimpleFileOptions::default();
    for (nama, data) in [
        ("[Content_Types].xml", "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/></Types>".to_string()),
        ("_rels/.rels", "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/></Relationships>".to_string()),
        ("word/document.xml", dokumen),
    ] {
        z.start_file(nama, o).map_err(|e| e.to_string())?;
        z.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
    }
    z.finish().map_err(|e| e.to_string())?;
    Ok(())
}

fn latin(s: &str) -> Vec<u8> {
    let mut o = vec![];
    for c in s.chars() {
        let t: &str = match c {
            '•' | '▪' | '·' => "-",
            '→' | '➜' => "->",
            '≈' => "~",
            '–' | '—' => "-",
            '“' | '”' => "\"",
            '‘' | '’' => "'",
            '✔' | '✅' => "[v]",
            '✖' | '❌' => "[x]",
            _ => "",
        };
        if !t.is_empty() {
            o.extend_from_slice(t.as_bytes());
        } else if (c as u32) < 256 && !c.is_control() {
            o.push(c as u8);
        } else if c.is_ascii() || c == '\t' {
            o.push(b' ');
        }
        // emoji dan aksara lain dibuang
    }
    o
}

fn bungkus(teks: &str, lebar: usize) -> Vec<String> {
    let mut hasil = vec![];
    for baris in teks.lines() {
        let mut cur = String::new();
        for kata in baris.split(' ') {
            if !cur.is_empty() && cur.chars().count() + 1 + kata.chars().count() > lebar {
                hasil.push(std::mem::take(&mut cur));
            }
            if !cur.is_empty() {
                cur.push(' ');
            }
            cur.push_str(kata);
        }
        hasil.push(cur);
    }
    hasil
}

/// Judul + baris teks → PDF A4 (Helvetica 10 pt, 55 baris per halaman).
pub fn tulis_pdf(path: &Path, judul: &str, baris: &[String]) -> Result<(), String> {
    let mut semua: Vec<String> = vec![judul.to_string(), String::new()];
    for b in baris {
        semua.extend(bungkus(b.trim_start_matches('#').trim_end(), 95));
    }
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font = doc.add_object(dictionary! {"Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding"});
    let res = doc.add_object(dictionary! {"Font" => dictionary! {"F1" => font}});
    let mut kids: Vec<Object> = vec![];
    for halaman in semua.chunks(55) {
        let mut c: Vec<u8> = b"BT /F1 10 Tf 50 800 Td 14 TL\n".to_vec();
        for l in halaman {
            c.push(b'(');
            for b in latin(l) {
                if matches!(b, b'(' | b')' | b'\\') {
                    c.push(b'\\');
                }
                c.push(b);
            }
            c.extend_from_slice(b") Tj T*\n");
        }
        c.extend_from_slice(b"ET");
        let cid = doc.add_object(Stream::new(dictionary! {}, c));
        kids.push(doc.add_object(dictionary! {"Type" => "Page", "Parent" => pages_id, "Contents" => cid}).into());
    }
    let n = kids.len() as i64;
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! {"Type" => "Pages", "Kids" => kids, "Count" => n, "Resources" => res, "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()]}));
    let cat = doc.add_object(dictionary! {"Type" => "Catalog", "Pages" => pages_id});
    doc.trailer.set("Root", cat);
    doc.save(path).map(|_| ()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docx_valid_dan_berisi_teks_escape() {
        let p = std::env::temp_dir().join("nova-uji.docx");
        tulis_docx(&p, "Laporan <uji>", &["# Bagian".into(), "isi & angka 5 > 3".into()]).unwrap();
        let mut z = zip::ZipArchive::new(std::fs::File::open(&p).unwrap()).unwrap();
        let mut s = String::new();
        std::io::Read::read_to_string(&mut z.by_name("word/document.xml").unwrap(), &mut s).unwrap();
        assert!(s.contains("Laporan &lt;uji&gt;") && s.contains("isi &amp; angka 5 &gt; 3") && z.by_name("[Content_Types].xml").is_ok());
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn pdf_terbaca_dan_terpaginasi() {
        let p = std::env::temp_dir().join("nova-uji.pdf");
        let baris: Vec<String> = (0..130).map(|i| format!("baris {i} (tanda kurung) ✔ → ok")).collect();
        tulis_pdf(&p, "Laporan uji", &baris).unwrap();
        let d = Document::load(&p).unwrap();
        assert_eq!(d.get_pages().len(), 3);
        let t = d.extract_text(&[1]).unwrap_or_default();
        assert!(t.contains("Laporan uji") || t.contains("baris"), "{t:?}");
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn bungkus_baris_panjang() {
        assert!(bungkus(&"kata ".repeat(60), 40).iter().all(|l| l.chars().count() <= 40));
    }
}
