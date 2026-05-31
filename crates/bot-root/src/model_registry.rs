//! model-registry — table modèle ↔ tier ↔ bot (décision D10).
//!
//! Encode le routage **adaptatif par-bot** avec **escalade intra-famille** :
//! chaque bot a un modèle par défaut (full GPU, rapide) et, optionnellement, un
//! modèle d'escalade plus gros (RAM-offload, lent mais exact). L'escalade reste
//! dans la même famille pour préserver la persona.

use std::collections::HashMap;

// `Bot` et `Tier` sont définis dans la crate partagée fleet-protocol (types de
// fil) et ré-exportés ici pour la commodité des consommateurs du registre.
pub use fleet_protocol::{Bot, Tier};

/// Modèles d'un bot : défaut + escalade intra-famille optionnelle.
#[derive(Debug, Clone)]
pub struct BotModels {
    /// Modèle par défaut (full GPU).
    pub default_model: String,
    /// Modèle d'escalade (`None` = pas d'escalade, ex. Wall-AI).
    pub escalation_model: Option<String>,
}

impl BotModels {
    /// Construit les modèles d'un bot.
    pub fn new(default_model: impl Into<String>, escalation_model: Option<String>) -> Self {
        Self {
            default_model: default_model.into(),
            escalation_model,
        }
    }

    /// Vrai si le bot dispose d'un modèle d'escalade.
    pub fn can_escalate(&self) -> bool {
        self.escalation_model.is_some()
    }

    /// Nom de modèle pour un tier.
    ///
    /// `Escalation` retombe sur le modèle par défaut si aucune escalade n'est
    /// configurée — un bot ne peut jamais router vers un modèle inexistant.
    pub fn model_for(&self, tier: Tier) -> &str {
        match tier {
            Tier::Default => &self.default_model,
            Tier::Escalation => self.escalation_model.as_deref().unwrap_or(&self.default_model),
        }
    }
}

/// Registre des modèles de la flotte.
#[derive(Debug, Default)]
pub struct ModelRegistry {
    bots: HashMap<Bot, BotModels>,
}

impl ModelRegistry {
    /// Crée un registre vide.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enregistre (ou remplace) les modèles d'un bot.
    pub fn register(&mut self, bot: Bot, models: BotModels) {
        self.bots.insert(bot, models);
    }

    /// Modèles d'un bot, s'il est enregistré.
    pub fn models_for(&self, bot: Bot) -> Option<&BotModels> {
        self.bots.get(&bot)
    }

    /// Résout le nom de modèle pour `(bot, tier)`. `None` si le bot est inconnu.
    pub fn resolve(&self, bot: Bot, tier: Tier) -> Option<&str> {
        self.models_for(bot).map(|m| m.model_for(tier))
    }

    /// Registre par défaut de la flotte (table D10).
    ///
    /// `winston_escalation` est passé en paramètre car il dépend de la
    /// disponibilité de `qwen2.5:14b` sur srv-bot : `Some("qwen2.5:14b")` si
    /// tiré, `None` pour plafonner Winston à `qwen3:8b`.
    pub fn fleet_defaults(winston_escalation: Option<String>) -> Self {
        let mut registry = Self::new();
        // Wall-AI : charge légère, jamais d'escalade.
        registry.register(Bot::WallAi, BotModels::new("qwen2.5:3b", None));
        // Nori-IA : mistral, escalade intra-famille vers NeMo 12b.
        registry.register(
            Bot::Noria,
            BotModels::new("noria:latest", Some("mistral-nemo:12b".to_owned())),
        );
        // Winston : qwen3:8b plancher, escalade qwen 14b si disponible.
        registry.register(Bot::Winston, BotModels::new("qwen3:8b", winston_escalation));
        registry
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_for_default_and_escalation() {
        let m = BotModels::new("qwen3:8b", Some("qwen2.5:14b".to_owned()));
        assert_eq!(m.model_for(Tier::Default), "qwen3:8b");
        assert_eq!(m.model_for(Tier::Escalation), "qwen2.5:14b");
        assert!(m.can_escalate());
    }

    #[test]
    fn escalation_falls_back_when_absent() {
        let m = BotModels::new("qwen2.5:3b", None);
        assert!(!m.can_escalate());
        // Pas d'escalade -> on reste sur le défaut (jamais de modèle inexistant).
        assert_eq!(m.model_for(Tier::Escalation), "qwen2.5:3b");
    }

    #[test]
    fn fleet_defaults_values() {
        let r = ModelRegistry::fleet_defaults(Some("qwen2.5:14b".to_owned()));
        assert_eq!(r.resolve(Bot::WallAi, Tier::Default), Some("qwen2.5:3b"));
        assert_eq!(r.resolve(Bot::WallAi, Tier::Escalation), Some("qwen2.5:3b")); // pas d'escalade
        assert_eq!(r.resolve(Bot::Noria, Tier::Default), Some("noria:latest"));
        assert_eq!(r.resolve(Bot::Noria, Tier::Escalation), Some("mistral-nemo:12b"));
        assert_eq!(r.resolve(Bot::Winston, Tier::Escalation), Some("qwen2.5:14b"));
    }

    #[test]
    fn winston_caps_at_default_when_no_escalation() {
        let r = ModelRegistry::fleet_defaults(None);
        assert_eq!(r.resolve(Bot::Winston, Tier::Default), Some("qwen3:8b"));
        assert_eq!(r.resolve(Bot::Winston, Tier::Escalation), Some("qwen3:8b"));
    }

    #[test]
    fn unknown_bot_resolves_none() {
        let r = ModelRegistry::new();
        assert_eq!(r.resolve(Bot::Winston, Tier::Default), None);
    }
}
