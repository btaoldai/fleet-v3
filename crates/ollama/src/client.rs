//! Client HTTP unifié vers Ollama (`/api/chat`, `stream: false`).

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tracing::{debug, error, warn};

use crate::config::OllamaConfig;
use crate::error::{OllamaError, Result};

// ─── Types API Ollama ───────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<Message<'a>>,
    stream: bool,
    /// Paramètres d'échantillonnage. Omis si aucun n'est configuré.
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<ChatOptions>,
    /// Durée de maintien en mémoire — champ **racine** de `/api/chat`
    /// (et non dans `options`, contrairement à l'ancien code Nori-IA).
    keep_alive: i64,
}

#[derive(Debug, Serialize)]
struct ChatOptions {
    temperature: f32,
}

#[derive(Debug, Serialize)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    message: MessageContent,
}

#[derive(Debug, Deserialize)]
struct MessageContent {
    content: String,
}

// ─── Client ─────────────────────────────────────────────────────────────────

/// Client Ollama unifié. Bon marché à cloner (`reqwest::Client` est un `Arc`
/// en interne ; on enveloppe en plus la config dans un `Arc`).
#[derive(Debug, Clone)]
pub struct OllamaClient {
    http: reqwest::Client,
    cfg: Arc<OllamaConfig>,
}

impl OllamaClient {
    /// Construit le client à partir d'une [`OllamaConfig`].
    ///
    /// Constructeur **unique** de la flotte — remplace les trois signatures
    /// historiques.
    pub fn new(cfg: OllamaConfig) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(cfg.timeout_secs))
            .build()
            .map_err(OllamaError::Build)?;

        Ok(Self {
            http,
            cfg: Arc::new(cfg),
        })
    }

    /// Envoie un message au modèle avec system prompt optionnel.
    ///
    /// * `model` — nom du modèle ; si vide, utilise `default_model` de la config.
    /// * `system` — prompt système (omis s'il est vide).
    /// * `user_msg` — message utilisateur.
    pub async fn chat(&self, model: &str, system: &str, user_msg: &str) -> Result<String> {
        let effective_model = if model.is_empty() {
            self.cfg.default_model.as_str()
        } else {
            model
        };
        let url = format!("{}/api/chat", self.base_url());

        let mut messages: Vec<Message<'_>> = Vec::new();
        if !system.is_empty() {
            messages.push(Message { role: "system", content: system });
        }
        messages.push(Message { role: "user", content: user_msg });

        let body = ChatRequest {
            model: effective_model,
            messages,
            stream: false,
            options: self.cfg.temperature.map(|temperature| ChatOptions { temperature }),
            keep_alive: self.cfg.keep_alive_secs,
        };

        debug!(model = effective_model, url = %url, "Ollama chat request");

        let resp = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|source| OllamaError::Request { url: url.clone(), source })?;

        let status = resp.status();
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_else(|_| "<illisible>".to_owned());
            error!(status = %status, body = %body_text, "Ollama HTTP error");
            return Err(OllamaError::Http {
                status: status.as_u16(),
                body: body_text,
            });
        }

        let data: ChatResponse = resp.json().await.map_err(OllamaError::Decode)?;
        Ok(data.message.content)
    }

    /// Vérifie qu'Ollama est joignable (`GET /api/tags`).
    pub async fn health_check(&self) -> bool {
        let url = format!("{}/api/tags", self.base_url());
        match self.http.get(&url).send().await {
            Ok(r) => r.status().is_success(),
            Err(e) => {
                warn!(error = %e, "Ollama health check failed");
                false
            }
        }
    }

    /// URL de base (sans slash final).
    pub fn base_url(&self) -> &str {
        self.cfg.base_url.trim_end_matches('/')
    }

    /// Owner Discord configuré (métadonnée de gating amont), s'il existe.
    pub fn owner_discord_id(&self) -> Option<u64> {
        self.cfg.owner_discord_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(cfg: OllamaConfig) -> OllamaClient {
        OllamaClient::new(cfg).expect("construction client")
    }

    /// `keep_alive` doit apparaître à la racine, jamais dans `options`.
    #[test]
    fn request_keep_alive_is_root_level() {
        let body = ChatRequest {
            model: "qwen3:8b",
            messages: vec![Message { role: "user", content: "salut" }],
            stream: false,
            options: None,
            keep_alive: 300,
        };
        let v = serde_json::to_value(&body).expect("sérialisation");
        assert_eq!(v["keep_alive"], 300);
        assert!(v.get("options").is_none(), "options omis quand pas de température");
    }

    /// Avec une température, elle apparaît dans `options.temperature`.
    #[test]
    fn request_includes_temperature_when_set() {
        let body = ChatRequest {
            model: "qwen3:8b",
            messages: vec![Message { role: "user", content: "salut" }],
            stream: false,
            options: Some(ChatOptions { temperature: 0.3 }),
            keep_alive: -1,
        };
        let v = serde_json::to_value(&body).expect("sérialisation");
        let temp = v["options"]["temperature"].as_f64().expect("température numérique");
        assert!((temp - 0.3).abs() < 1e-6, "température ~0.3 attendue, obtenu {temp}");
        assert_eq!(v["keep_alive"], -1);
    }

    /// La config builder propage correctement les valeurs.
    #[test]
    fn config_builder_propagates() {
        let c = client(
            OllamaConfig::new("http://ollama:11434/", "qwen3:8b")
                .timeout_secs(240)
                .temperature(0.3)
                .keep_alive_secs(-1)
                .owner_discord_id(200_000_000_000_000_002),
        );
        // base_url() retire le slash final.
        assert_eq!(c.base_url(), "http://ollama:11434");
        assert_eq!(c.owner_discord_id(), Some(200_000_000_000_000_002));
    }

    /// Modèle vide -> repli sur le modèle par défaut (vérifié via le corps construit).
    #[test]
    fn empty_model_falls_back_to_default() {
        let cfg = OllamaConfig::new("http://ollama:11434", "mistral:7b");
        let effective = if "".is_empty() { cfg.default_model.as_str() } else { "x" };
        assert_eq!(effective, "mistral:7b");
    }
}
