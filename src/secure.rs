//! Penyimpanan rahasia terenkripsi per-pengguna Windows (DPAPI) lewat PowerShell: token OAuth dan sandi aplikasi email
//! tidak lagi disimpan sebagai teks biasa. Hanya akun Windows Bos di laptop ini yang bisa membukanya.
use std::collections::HashMap;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::Mutex;

static CACHE: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

fn run_ps(script: &str, input: &str) -> Option<String> {
    let mut c = Command::new("powershell");
    c.args(["-NoProfile", "-NonInteractive", "-Command", script]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let mut child = c.spawn().ok()?;
    child.stdin.take()?.write_all(input.as_bytes()).ok()?;
    let out = child.wait_with_output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string()).filter(|s| !s.is_empty())
}

/// Enkripsi teks -> string heksa DPAPI (rahasia dikirim lewat stdin, tidak lewat argumen perintah).
pub fn protect(plain: &str) -> Option<String> {
    run_ps("$s=[Console]::In.ReadToEnd(); ConvertTo-SecureString -String $s -AsPlainText -Force | ConvertFrom-SecureString", plain)
}

/// Dekripsi string heksa DPAPI; hasil dicache di RAM selama proses berjalan.
pub fn unprotect(enc: &str) -> Option<String> {
    if let Some(v) = CACHE.lock().ok()?.as_ref().and_then(|m| m.get(enc)) {
        return Some(v.clone());
    }
    let v = run_ps("$h=[Console]::In.ReadToEnd().Trim(); $ss=ConvertTo-SecureString $h; [Runtime.InteropServices.Marshal]::PtrToStringAuto([Runtime.InteropServices.Marshal]::SecureStringToBSTR($ss))", enc)?;
    CACHE.lock().ok()?.get_or_insert_with(HashMap::new).insert(enc.to_string(), v.clone());
    Some(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dpapi_roundtrip() {
        let enc = protect("rahasia-uji-123").expect("DPAPI tersedia di Windows");
        assert!(!enc.contains("rahasia"));
        assert_eq!(unprotect(&enc).as_deref(), Some("rahasia-uji-123"));
    }
}
