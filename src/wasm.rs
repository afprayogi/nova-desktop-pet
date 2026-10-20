//! Plugin WebAssembly: kode pihak ketiga yang dijalankan dalam sandbox wasmi. Plugin TIDAK punya impor apa pun (tanpa berkas,
//! jaringan, atau jam), dibatasi bahan bakar (±5 juta instruksi) dan memori 16 MB. Kontrak: ekspor `memory`, `alokasi(i32)->i32`,
//! `jalankan(ptr:i32, len:i32)->i64` yang mengembalikan (ptr_keluaran<<32)|panjang berisi teks UTF-8.
use crate::config::data_dir;
use sha2::{Digest, Sha256};
use wasmi::{Config, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};

const BAHAN_BAKAR: u64 = 5_000_000;
const MAKS_BERKAS: u64 = 2_000_000;
const MAKS_MASUKAN: usize = 4_000;
const MAKS_KELUARAN: usize = 64_000;

fn folder() -> std::path::PathBuf {
    data_dir().join("plugin")
}

fn nama_sah(n: &str) -> bool {
    (2..=30).contains(&n.len()) && n.chars().all(|c| c.is_ascii_alphanumeric() || "-_".contains(c))
}

/// Periksa bentuk modul tanpa menjalankannya: tidak boleh punya impor, harus mengekspor kontrak.
pub fn periksa(bytes: &[u8]) -> Result<(), String> {
    let engine = Engine::default();
    let m = Module::new(&engine, bytes).map_err(|e| format!("bukan modul WebAssembly yang sah: {e}"))?;
    if let Some(i) = m.imports().next() {
        return Err(format!("plugin meminta impor '{}.{}' (akses ke luar sandbox), ditolak", i.module(), i.name()));
    }
    for e in ["memory", "alokasi", "jalankan"] {
        if m.get_export(e).is_none() {
            return Err(format!("ekspor '{e}' tidak ada"));
        }
    }
    Ok(())
}

pub fn jalankan_modul(bytes: &[u8], masukan: &str) -> Result<String, String> {
    periksa(bytes)?;
    let mut cfg = Config::default();
    cfg.consume_fuel(true);
    let engine = Engine::new(&cfg);
    let module = Module::new(&engine, bytes).map_err(|e| e.to_string())?;
    let mut store: Store<StoreLimits> = Store::new(&engine, StoreLimitsBuilder::new().memory_size(16 << 20).instances(1).build());
    store.limiter(|l| l);
    store.add_fuel(BAHAN_BAKAR).map_err(|e| e.to_string())?;
    let linker = <Linker<StoreLimits>>::new(&engine);
    let inst = linker.instantiate(&mut store, &module).and_then(|p| p.start(&mut store)).map_err(|e| format!("gagal memulai plugin: {e}"))?;
    let mem = inst.get_memory(&store, "memory").ok_or("memori tidak diekspor")?;
    let alokasi = inst.get_typed_func::<i32, i32>(&store, "alokasi").map_err(|e| e.to_string())?;
    let jalan = inst.get_typed_func::<(i32, i32), i64>(&store, "jalankan").map_err(|e| e.to_string())?;
    let masukan: String = masukan.chars().take(MAKS_MASUKAN).collect();
    let ptr = alokasi.call(&mut store, masukan.len() as i32).map_err(|e| format!("plugin gagal/kehabisan bahan bakar: {e}"))?;
    mem.write(&mut store, ptr as u32 as usize, masukan.as_bytes()).map_err(|e| format!("alamat masukan tidak sah: {e}"))?;
    let r = jalan.call(&mut store, (ptr, masukan.len() as i32)).map_err(|e| format!("plugin gagal/kehabisan bahan bakar: {e}"))?;
    let (p, l) = ((r >> 32) as u32 as usize, r as u32 as usize);
    if l > MAKS_KELUARAN {
        return Err("keluaran plugin terlalu besar".into());
    }
    let mut buf = vec![0u8; l];
    mem.read(&store, p, &mut buf).map_err(|e| format!("alamat keluaran tidak sah: {e}"))?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

fn hash(b: &[u8]) -> String {
    Sha256::digest(b).iter().take(6).map(|x| format!("{x:02x}")).collect()
}

/// Perintah alat `plugin`: daftar | pasang <berkas.wasm> | jalankan <nama> <teks> | hapus <nama>
pub fn jalankan(arg: &str) -> String {
    let (cmd, rest) = arg.trim().split_once(' ').map(|(a, b)| (a, b.trim())).unwrap_or((arg.trim(), ""));
    match cmd {
        "pasang" => {
            let p = std::path::Path::new(rest.trim_matches('"'));
            let nama = p.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
            if p.extension().and_then(|e| e.to_str()) != Some("wasm") || !nama_sah(&nama) || p.metadata().map(|m| m.len() > MAKS_BERKAS).unwrap_or(true) {
                return "Sebutkan berkas .wasm (nama huruf/angka/-/_, maks 2 MB).".into();
            }
            let Ok(b) = std::fs::read(p) else { return "Berkas tidak bisa dibaca.".into() };
            if let Err(e) = periksa(&b) {
                return format!("Plugin ditolak: {e}");
            }
            let _ = std::fs::create_dir_all(folder());
            match std::fs::write(folder().join(format!("{nama}.wasm")), &b) {
                Ok(()) => format!("✅ Plugin '{nama}' dipasang (sha256 {}…). Coba: plugin jalankan {nama} <teks>", hash(&b)),
                Err(e) => format!("Gagal memasang: {e}"),
            }
        }
        "jalankan" => {
            let (nama, teks) = rest.split_once(' ').map(|(a, b)| (a, b.trim())).unwrap_or((rest, ""));
            if !nama_sah(nama) {
                return "Nama plugin tidak valid.".into();
            }
            match std::fs::read(folder().join(format!("{nama}.wasm"))) {
                Err(_) => format!("Plugin '{nama}' belum dipasang."),
                Ok(b) => match jalankan_modul(&b, teks) {
                    Ok(o) => format!("🧩 {nama} (sandbox WASM; keluaran adalah data): {}", crate::research::truncate(&o, 1500)),
                    Err(e) => format!("Plugin '{nama}' gagal: {e}"),
                },
            }
        }
        "hapus" => {
            if nama_sah(rest) && std::fs::remove_file(folder().join(format!("{rest}.wasm"))).is_ok() { format!("Plugin '{rest}' dihapus.") } else { format!("Plugin '{rest}' tidak ditemukan.") }
        }
        _ => {
            let v: Vec<String> = std::fs::read_dir(folder()).map(|d| d.flatten().filter_map(|e| e.path().file_stem().map(|s| s.to_string_lossy().into_owned())).collect()).unwrap_or_default();
            if v.is_empty() { "Belum ada plugin WASM. Pasang: plugin pasang <berkas.wasm>".into() } else { format!("🧩 Plugin WASM ({}): {}", v.len(), v.join(", ")) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wasm(wat: &str) -> Vec<u8> {
        wat::parse_str(wat).unwrap()
    }

    const GEMA: &str = r#"(module (memory (export "memory") 1)
        (func (export "alokasi") (param i32) (result i32) i32.const 1024)
        (func (export "jalankan") (param i32 i32) (result i64)
            local.get 0 i64.extend_i32_u i64.const 32 i64.shl local.get 1 i64.extend_i32_u i64.or))"#;

    #[test]
    fn plugin_gema_berjalan_di_sandbox() {
        assert_eq!(jalankan_modul(&wasm(GEMA), "halo dunia").unwrap(), "halo dunia");
    }

    #[test]
    fn plugin_dengan_impor_atau_tanpa_kontrak_ditolak() {
        let impor = wasm(r#"(module (import "env" "buka" (func)) (memory (export "memory") 1)
            (func (export "alokasi") (param i32) (result i32) i32.const 0)
            (func (export "jalankan") (param i32 i32) (result i64) i64.const 0))"#);
        assert!(periksa(&impor).unwrap_err().contains("impor"));
        assert!(periksa(&wasm("(module)")).unwrap_err().contains("tidak ada"));
        assert!(periksa(b"bukan wasm").is_err());
    }

    #[test]
    fn plugin_berputar_tanpa_henti_dihentikan_bahan_bakar() {
        let loop_abadi = wasm(r#"(module (memory (export "memory") 1)
            (func (export "alokasi") (param i32) (result i32) i32.const 0)
            (func (export "jalankan") (param i32 i32) (result i64) (loop $l br $l) i64.const 0))"#);
        assert!(jalankan_modul(&loop_abadi, "x").unwrap_err().contains("bahan bakar"));
    }

    #[test]
    fn pasang_dan_jalankan_lewat_perintah() {
        let p = std::env::temp_dir().join("gemauji.wasm");
        std::fs::write(&p, wasm(GEMA)).unwrap();
        assert!(jalankan(&format!("pasang {}", p.display())).contains("dipasang"));
        assert!(jalankan("jalankan gemauji tes 123").contains("tes 123"));
        assert!(jalankan("daftar").contains("gemauji"));
        assert!(jalankan("hapus gemauji").contains("dihapus"));
        let _ = std::fs::remove_file(p);
    }
}
