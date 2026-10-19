//! Kru (pola CrewAI): manajer memecah tujuan menjadi tugas berurutan; tiap tugas dikerjakan satu peran (peneliti, pelaksana,
//! penulis) lewat loop ReAct terbatas (pikir → alat → amati), dan hasilnya menjadi konteks tugas berikutnya. Semua alat tetap
//! melewati `alat::assess`: yang aman dijalankan, yang butuh persetujuan TIDAK dijalankan kru (dilaporkan ke Bos), yang berbahaya diblokir.
#[cfg(test)]
use serde_json::json;
use super::*;

pub const PERAN_BAWAAN: &str = include_str!("../../data/peran.json");

/// (nama, judul, tugas, latar) dari peran.json (bawaan + peran tambahan Bos).
fn daftar_peran() -> Vec<(String, String, String, String)> {
    crate::data::lapis("peran.json", PERAN_BAWAAN)
        .as_array()
        .map(|a| a.iter().filter_map(|x| Some((x["nama"].as_str()?.to_lowercase(), x["judul"].as_str().unwrap_or("").to_string(), x["tugas"].as_str().unwrap_or("").to_string(), x["latar"].as_str().unwrap_or("").to_string()))).collect())
        .unwrap_or_default()
}

/// Boleh tidaknya `peran` memakai `tool` menurut `alat_boleh` / `alat_dilarang` di peran.json (kosong = bebas).
fn izin(peran: &str, tool: &str) -> bool {
    let d = crate::data::lapis("peran.json", PERAN_BAWAAN);
    let Some(x) = d.as_array().and_then(|a| a.iter().find(|x| x["nama"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(peran)))) else { return true };
    let daftar = |k: &str| -> Vec<String> { x[k].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(|s| s.to_lowercase())).collect() };
    let (boleh, larang) = (daftar("alat_boleh"), daftar("alat_dilarang"));
    !larang.iter().any(|t| t == tool) && (boleh.is_empty() || boleh.iter().any(|t| t == tool))
}

fn planner() -> String {
    let r: Vec<String> = daftar_peran().iter().map(|(n, _, t, _)| format!("{n} ({t})")).collect();
    format!("Kamu manajer kru asisten laptop. Pecah tujuan menjadi 2-4 tugas BERURUTAN (hasil tugas awal dipakai tugas berikutnya). Peran: {}. Peran penulis selalu terakhir. Balas HANYA JSON: {{\"tugas\":[{{\"peran\":\"peneliti\",\"deskripsi\":\"...\",\"hasil\":\"hasil yang diharapkan\"}}]}}", r.join(", "))
}

/// Ambil objek JSON pertama dari balasan model (toleran terhadap teks di sekitarnya).
pub(super) fn ambil_json(raw: &str) -> Option<Value> {
    let (a, b) = (raw.find('{')?, raw.rfind('}')?);
    serde_json::from_str(raw.get(a..=b)?).ok()
}

/// (peran, deskripsi, hasil) dari balasan perencana; peran tak dikenal menjadi pelaksana; maks 4 tugas.
pub(super) fn urai_rencana(v: &Value) -> Vec<(String, String, String)> {
    v["tugas"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|t| {
                    let d = t["deskripsi"].as_str()?.trim().to_string();
                    let p = t["peran"].as_str().unwrap_or("pelaksana").to_lowercase();
                    let p = if daftar_peran().iter().any(|(k, _, _, _)| *k == p) { p } else { "pelaksana".into() };
                    (!d.is_empty()).then(|| (p, d, t["hasil"].as_str().unwrap_or("").to_string()))
                })
                .take(4)
                .collect()
        })
        .unwrap_or_default()
}

async fn tanya_json(http: &reqwest::Client, sys: &str, user: &str) -> Option<Value> {
    for percobaan in 0..2 {
        let u = if percobaan == 0 { user.to_string() } else { format!("{user}\n\nBalasan sebelumnya bukan JSON yang valid. Balas HANYA satu objek JSON.") };
        let raw = match crate::gemini::generate(http, sys, &u, true, false).await {
            Ok(r) => r,
            Err(_) => crate::lokal::chat(http, &format!("{sys}\n\n{u}")).await.unwrap_or_default(),
        };
        if let Some(v) = ambil_json(&raw) {
            return Some(v);
        }
    }
    None
}

impl Engine {
    pub(super) async fn kru(&mut self, tujuan: &str) {
        let http = self.http.clone();
        let alat_txt: String = crate::registry::bantuan().lines().map(|l| l.chars().take(120).collect::<String>()).collect::<Vec<_>>().join("\n");
        self.tx.send(Event::Thinking(Some(0)));
        self.agent_say(0, &format!("🧑‍🤝‍🧑 Kru bekerja untuk: {tujuan}"));
        let mut tugas = tanya_json(&http, &planner(), &format!("Tujuan: {tujuan}")).await.map(|v| urai_rencana(&v)).unwrap_or_default();
        if tugas.is_empty() {
            tugas = vec![("pelaksana".into(), tujuan.to_string(), "jawaban untuk tujuan".into())];
        }
        self.agent_say(0, &format!("Rencana: {}", tugas.iter().enumerate().map(|(i, (p, d, _))| format!("{}. {p}: {d}", i + 1)).collect::<Vec<_>>().join(" → ")));
        let (mut konteks, mut butuh_izin, mut terakhir) = (String::new(), Vec::<String>::new(), String::new());
        for (peran, desc, hasil) in &tugas {
            let roles = daftar_peran();
            let (nama, latar) = roles.iter().find(|r| r.0 == *peran).or_else(|| roles.iter().find(|r| r.0 == "pelaksana")).map(|r| (r.1.clone(), r.3.clone())).unwrap_or_default();
            let (nama, latar) = (nama.as_str(), latar.as_str());
            self.agent_say(0, &format!("▶ [{nama}] {desc}"));
            let (mut obs, mut dipakai, mut jawab) = (String::new(), HashSet::<(String, String)>::new(), String::new());
            for _ in 0..4 {
                let sys = format!("Kamu {nama}. {latar}\nAlat yang ada:\n{alat_txt}\nBalas HANYA JSON: {{\"pikir\":\"...\",\"tool\":\"nama\",\"arg\":\"...\"}} untuk memakai SATU alat, atau {{\"final\":\"hasil tugas\"}} bila sudah cukup. Jangan mengulang alat dengan argumen yang sama; bila hasil alat sudah menjawab, beri final.");
                let user = format!("Tujuan besar: {tujuan}\nTugasmu: {desc}\nHasil yang diharapkan: {hasil}\n\nKonteks dari tugas sebelumnya:\n{konteks}\nRiwayat alat tugas ini:\n{obs}");
                let Some(v) = tanya_json(&http, &sys, &user).await else { break };
                if let Some(f) = v["final"].as_str() {
                    jawab = f.to_string();
                    break;
                }
                let (tool, arg) = (v["tool"].as_str().unwrap_or("").trim().to_string(), v["arg"].as_str().unwrap_or("").trim().to_string());
                if tool.is_empty() {
                    jawab = research::truncate(&v.to_string(), 600);
                    break;
                }
                if peran == "penulis" || !dipakai.insert((tool.clone(), arg.clone())) {
                    obs.push_str("(tidak bisa: penulis tanpa alat, atau alat+argumen itu sudah dipakai — beri final)\n");
                    continue;
                }
                if !izin(peran, &tool) {
                    obs.push_str(&format!("alat {tool} tidak diizinkan untuk peran {peran} (peran.json); pakai alat lain atau beri final.\n"));
                    continue;
                }
                let call = Call { tool: tool.clone(), arg: arg.clone() };
                match alat::assess(&call) {
                    Risk::Safe => {
                        let r = alat::run(&call, &http).await;
                        self.emit_term(&call, &r);
                        obs.push_str(&format!("{tool} {arg} => {}\n", research::truncate(&r, 1500)));
                    }
                    Risk::Confirm(why) => {
                        butuh_izin.push(format!("{tool} {arg}"));
                        obs.push_str(&format!("{tool} butuh persetujuan Bos ({why}); kru tidak menjalankannya.\n"));
                    }
                    Risk::Block(why) => obs.push_str(&format!("{tool} diblokir: {why}\n")),
                }
            }
            if jawab.is_empty() {
                jawab = if obs.is_empty() { "(tidak ada hasil)".into() } else { research::truncate(&obs, 800) };
            }
            konteks.push_str(&format!("[{nama}] {jawab}\n"));
            self.agent_say(0, &format!("✔ [{nama}] {}", research::truncate(&jawab, 500)));
            terakhir = jawab;
        }
        self.tx.send(Event::Thinking(None));
        let mut akhir = format!("Hasil kru:\n{terakhir}");
        if !butuh_izin.is_empty() {
            akhir.push_str(&format!("\n\nTindakan yang butuh persetujuanmu (tidak dijalankan kru): {}. Ketik perintahnya sendiri bila setuju.", butuh_izin.join("; ")));
        }
        self.agent_say(0, &akhir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_plan_and_json() {
        let v = ambil_json("Ini rencananya: {\"tugas\":[{\"peran\":\"Peneliti\",\"deskripsi\":\"cari harga\",\"hasil\":\"angka\"},{\"peran\":\"aneh\",\"deskripsi\":\"catat\"},{\"deskripsi\":\"\"},{\"peran\":\"penulis\",\"deskripsi\":\"rangkum\"}]} selesai").unwrap();
        let t = urai_rencana(&v);
        assert_eq!(t.len(), 3);
        assert_eq!(t[0].0, "peneliti");
        assert_eq!(t[1].0, "pelaksana");
        assert_eq!(t[2].0, "penulis");
        assert!(ambil_json("tanpa json").is_none());
        let _ = json!({});
    }
}
