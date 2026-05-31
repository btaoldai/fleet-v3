//! Configuration du client Ollama unifié.

/// Configuration d'un [`crate::OllamaClient`].
///
/// Construite par builder pour absorber les besoins divergents des trois bots
/// sans multiplier les constructeurs (dette n°1 de l'audit).
///
/// # Exemple
/// ```
/// use fleet_ollama::OllamaConfig;
/// // Winston : modèle par défaut + température flotte (ADR-012).
/// let cfg = OllamaConfig::new("http://ollama:11434", "qwen3:8b")
///     .timeout_secs(240)
///     .temperature(0.3);
/// assert_eq!(cfg.default_model, "qwen3:8b");
/// ```
#[derive(Debug, Clone)]
pub struct OllamaConfig {
    /// URL de base d'Ollama (ex. `http://ollama:11434`).
    pub base_url: String,
    /// Modèle utilisé quand l'appelant ne précise pas de modèle.
    pub default_model: String,
    /// Timeout HTTP en secondes (généreux pour l'inférence CPU / RAM-offload).
    pub timeout_secs: u64,
    /// Température d'échantillonnage. `None` = défaut du modèle (ADR-012 : 0.3).
    pub temperature: Option<f32>,
    /// Durée (s) de maintien du modèle en mémoire après requête. Défaut : 300.
    /// `-1` = indéfiniment, `0` = décharge immédiate.
    pub keep_alive_secs: i64,
    /// Identifiant owner pour le gating amont (porté en métadonnée ; le client
    /// ne l'applique pas lui-même — c'est le rôle des handlers).
    pub owner_discord_id: Option<u64>,
}

impl OllamaConfig {
    /// Crée une configuration avec les valeurs par défaut raisonnables
    /// (`timeout_secs = 120`, `keep_alive_secs = 300`, pas de température fixée,
    /// pas d'owner).
    pub fn new(base_url: impl Into<String>, default_model: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            default_model: default_model.into(),
            timeout_secs: 120,
            temperature: None,
            keep_alive_secs: 300,
            owner_discord_id: None,
        }
    }

    /// Fixe le timeout HTTP (secondes).
    pub fn timeout_secs(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Fixe la température d'échantillonnage.
    pub fn temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// Fixe la durée de maintien en mémoire (`keep_alive`).
    pub fn keep_alive_secs(mut self, secs: i64) -> Self {
        self.keep_alive_secs = secs;
        self
    }

    /// Renseigne l'owner Discord (métadonnée de gating amont).
    pub fn owner_discord_id(mut self, id: u64) -> Self {
        self.owner_discord_id = Some(id);
        self
    }
}
