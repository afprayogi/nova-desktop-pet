//! Server MCP (stdio) agar asisten lain (Claude Desktop dsb.) bisa memakai alat NOVA: `nova --mcp`.
//! Hanya alat yang dinilai AMAN yang dijalankan; yang butuh izin Bos ditolak (tidak ada layar persetujuan di jalur ini), dan alat yang
//! membuat otomasi/mengubah NOVA sendiri tidak ditawarkan sama sekali.
use crate::alat::{self, Call, Risk};
use serde_json::{json, Value};
use std::io::{BufRead, Write};

fn boleh(nama: &str) -> bool {
    !crate::penjadwal::DILARANG.contains(&nama) && crate::registry::ada(nama)
}

fn balas(id: &Value, hasil: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": hasil})
}

fn galat(id: &Value, kode: i32, pesan: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": kode, "message": pesan}})
}

/// Proses satu pesan JSON-RPC; None untuk notifikasi (tanpa balasan).
pub async fn tangani(http: &reqwest::Client, m: &Value) -> Option<Value> {
    let id = m.get("id")?.clone();
    Some(match m["method"].as_str().unwrap_or("") {
        "initialize" => balas(&id, json!({"protocolVersion": m["params"]["protocolVersion"].as_str().unwrap_or("2024-11-05"), "capabilities": {"tools": {}}, "serverInfo": {"name": "nova", "version": crate::pembaruan::VERSI}})),
        "ping" => balas(&id, json!({})),
        "tools/list" => {
            let mut l = crate::registry::mcp();
            if let Some(a) = l["tools"].as_array_mut() {
                a.retain(|t| t["name"].as_str().is_some_and(boleh));
            }
            balas(&id, l)
        }
        "tools/call" => {
            let nama = m["params"]["name"].as_str().unwrap_or("");
            let arg = m["params"]["arguments"]["arg"].as_str().unwrap_or("").to_string();
            if !boleh(nama) {
                return Some(galat(&id, -32602, "alat tidak tersedia"));
            }
            let c = Call { tool: nama.into(), arg };
            let (teks, err) = match alat::assess(&c) {
                Risk::Safe => (Box::pin(alat::run(&c, http)).await, false),
                Risk::Confirm(r) => (format!("Ditolak: butuh persetujuan Bos di aplikasi NOVA ({r})."), true),
                Risk::Block(r) => (format!("Diblokir: {r}"), true),
            };
            balas(&id, json!({"content": [{"type": "text", "text": teks}], "isError": err}))
        }
        _ => galat(&id, -32601, "metode tidak dikenal"),
    })
}

/// Titik masuk `nova --mcp`: baca JSON per baris dari stdin, tulis balasan per baris ke stdout.
pub fn serve() -> i32 {
    let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() else { return 1 };
    let http = crate::research::client(30);
    let mut out = std::io::stdout();
    for baris in std::io::stdin().lock().lines().map_while(Result::ok) {
        let Ok(m) = serde_json::from_str::<Value>(&baris) else { continue };
        if let Some(r) = rt.block_on(tangani(&http, &m)) {
            let _ = writeln!(out, "{r}");
            let _ = out.flush();
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_daftar_dan_batas_keamanan() {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let http = crate::research::client(5);
        let l = rt.block_on(tangani(&http, &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}))).unwrap();
        let nama: Vec<&str> = l["result"]["tools"].as_array().unwrap().iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(nama.contains(&"hitung") || nama.contains(&"baterai"));
        assert!(!nama.contains(&"jadwalkan") && !nama.contains(&"keahlian") && !nama.contains(&"skrip"));
        let k = rt.block_on(tangani(&http, &json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "jadwalkan", "arguments": {"arg": "daftar"}}}))).unwrap();
        assert!(k["error"].is_object());
        let sh = rt.block_on(tangani(&http, &json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "shell", "arguments": {"arg": "del x"}}}))).unwrap();
        assert!(sh["result"]["isError"] == json!(true) || sh["error"].is_object());
        assert!(rt.block_on(tangani(&http, &json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))).is_none());
    }
}
