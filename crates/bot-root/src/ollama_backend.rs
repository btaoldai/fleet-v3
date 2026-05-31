//! ollama_backend — adaptateur branchant `fleet-ollama` derrière le trait
//! [`InferenceBackend`] du slot-manager.
//!
//! C'est le seul point où bot-root parle réellement à Ollama ; tout passe par le
//! slot-manager qui le sérialise.

use async_trait::async_trait;
use fleet_ollama::OllamaClient;

use crate::slot_manager::{BackendError, InferenceBackend};

/// Backend d'inférence réel : un client Ollama unifié.
pub struct OllamaBackend {
    client: OllamaClient,
}

impl OllamaBackend {
    /// Enveloppe un [`OllamaClient`] déjà configuré.
    pub fn new(client: OllamaClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl InferenceBackend for OllamaBackend {
    async fn generate(&self, model: &str, system: &str, user: &str) -> Result<String, BackendError> {
        self.client
            .chat(model, system, user)
            .await
            .map_err(|e| BackendError(e.to_string()))
    }
}
