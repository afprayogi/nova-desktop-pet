//! Klasifikasi niat terstruktur: Gemini (JSON mode) → OmniRoute → Ollama lokal.
use anyhow::{anyhow, Result};
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "intent", rename_all = "snake_case")]
pub enum Intent {
    SetReminder { text: String, when: String },
    FeedPet { food: Option<String> },
    HealthQuery { question: String },
    GeneralChat { reply: String },
}

const SYS: &str = "Klasifikasikan pesan pengguna. Balas HANYA JSON: \
{\"intent\":\"set_reminder\",\"text\":..,\"when\":..} | {\"intent\":\"feed_pet\",\"food\":..|null} | \
{\"intent\":\"health_query\",\"question\":..} | {\"intent\":\"general_chat\",\"reply\":..}";

fn parse(raw: &str) -> Result<Intent> {
    let (a, b) = (raw.find('{'), raw.rfind('}'));
    match (a, b) {
        (Some(a), Some(b)) if b > a => Ok(serde_json::from_str(&raw[a..=b])?),
        _ => Err(anyhow!("bukan JSON: {raw}")),
    }
}

pub async fn classify(http: &reqwest::Client, msg: &str) -> Result<Intent> {
    match crate::gemini::generate(http, SYS, msg, true, false).await.and_then(|r| parse(&r)) {
        Ok(i) => Ok(i),
        Err(_) => parse(&crate::lokal::chat(http, &format!("{SYS}\n\nPesan: {msg}")).await?),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_variants() {
        assert_eq!(parse(r#"x {"intent":"feed_pet","food":null} y"#).unwrap(), Intent::FeedPet { food: None });
        assert!(matches!(parse(r#"{"intent":"set_reminder","text":"a","when":"5m"}"#).unwrap(), Intent::SetReminder { .. }));
        assert!(parse("halo").is_err());
    }
}
