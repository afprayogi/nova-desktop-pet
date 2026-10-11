use anyhow::{bail, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Clone, Serialize)]
pub struct Msg {
    pub role: String,
    pub content: String,
}

impl Msg {
    pub fn sys(s: impl Into<String>) -> Self {
        Self { role: "system".into(), content: s.into() }
    }
    pub fn user(s: impl Into<String>) -> Self {
        Self { role: "user".into(), content: s.into() }
    }
}

/// Klien OmniRoute (API kompatibel OpenAI).
#[derive(Clone)]
pub struct Omni {
    http: reqwest::Client,
    base: String,
    key: String,
}

impl Omni {
    pub fn new(base: &str, key: &str, timeout_s: u64) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_s.max(5)))
            .build()
            .expect("http client");
        Self { http, base: base.trim().to_string(), key: key.trim().to_string() }
    }

    /// Ollama lokal: pakai API native agar bisa membatasi thread, konteks, dan keep_alive (hemat CPU/RAM).
    fn is_ollama(&self) -> bool {
        self.key == "ollama" || self.base.contains(":11434")
    }

    fn root(&self) -> String {
        let b = self.base.trim_end_matches('/');
        b.strip_suffix("/v1").unwrap_or(b).to_string()
    }

    fn url(&self, path: &str) -> String {
        let b = self.base.trim_end_matches('/');
        let b = b.strip_suffix("/v1").unwrap_or(b);
        format!("{b}/v1/{path}")
    }

    pub async fn list_models(&self) -> Result<Vec<String>> {
        if self.is_ollama() {
            let v: Value = self.http.get(format!("{}/api/tags", self.root())).send().await?.json().await?;
            return Ok(v["models"].as_array().map(|a| a.iter().filter_map(|m| m["name"].as_str().map(String::from)).collect()).unwrap_or_default());
        }
        let resp = self.http.get(self.url("models")).bearer_auth(&self.key).send().await?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            bail!("HTTP {}: {}", status.as_u16(), text.chars().take(200).collect::<String>());
        }
        let v: Value = serde_json::from_str(&text)?;
        let arr = v["data"].as_array().or_else(|| v.as_array());
        let Some(arr) = arr else { bail!("format /models tidak dikenal") };
        Ok(arr
            .iter()
            .filter_map(|m| m["id"].as_str().or_else(|| m.as_str()).map(String::from))
            .collect())
    }

    pub async fn chat(&self, model: &str, msgs: &[Msg], max_tokens: u32, temp: f32) -> Result<String> {
        if self.is_ollama() {
            let body = json!({
                "model": model, "messages": msgs, "stream": false, "keep_alive": "2m",
                "options": { "num_ctx": 2048, "num_thread": crate::llm::threads(), "num_predict": max_tokens, "temperature": temp }
            });
            let resp = self.http.post(format!("{}/api/chat", self.root())).json(&body).send().await?;
            let status = resp.status();
            let text = resp.text().await?;
            if !status.is_success() {
                bail!("HTTP {}: {}", status.as_u16(), text.chars().take(200).collect::<String>());
            }
            let v: Value = serde_json::from_str(&text)?;
            let clean = strip_think(v["message"]["content"].as_str().unwrap_or(""));
            if clean.trim().is_empty() {
                bail!("jawaban kosong");
            }
            return Ok(clean.trim().to_string());
        }
        let body = json!({
            "model": model,
            "messages": msgs,
            "max_tokens": max_tokens,
            "temperature": temp,
            "stream": false
        });
        let resp = self
            .http
            .post(self.url("chat/completions"))
            .bearer_auth(&self.key)
            .json(&body)
            .send()
            .await?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            bail!("HTTP {}: {}", status.as_u16(), text.chars().take(200).collect::<String>());
        }
        let v: Value = serde_json::from_str(&text)?;
        let content = v["choices"][0]["message"]["content"].as_str().unwrap_or("");
        let clean = strip_think(content);
        if clean.trim().is_empty() {
            bail!("jawaban kosong");
        }
        Ok(clean.trim().to_string())
    }
}

/// Buang blok <think>...</think> dari model penalar.
pub fn strip_think(s: &str) -> String {
    let mut out = s.to_string();
    while let Some(i) = out.find("<think>") {
        match out[i..].find("</think>") {
            Some(j) => out.replace_range(i..i + j + "</think>".len(), ""),
            None => {
                out.truncate(i);
                break;
            }
        }
    }
    out
}
