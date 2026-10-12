//! Teks panduan dan bantuan NOVA: isinya ada di data/teks.json (bawaan, tertanam) dan otak/teks.json (tambahan/timpaan milik Bos).
pub(super) fn guide() -> String {
    crate::teknik::teks_json()["guide"].as_str().unwrap_or("").to_string()
}

pub(super) fn help() -> String {
    crate::teknik::teks_json()["help"].as_str().unwrap_or("").to_string()
}
