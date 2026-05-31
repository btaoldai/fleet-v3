//! fleet-protocol — types de fil partagés entre bot-root (daemon) et les bots
//! (clients).
//!
//! Pur serde, **sans tokio ni dépendance lourde** : un bot peut dépendre de ces
//! types sans tirer tout le daemon. Sérialisés en JSON ligne-à-ligne sur le
//! socket Unix (décision D8).

use serde::{Deserialize, Serialize};

/// Bot logique de la flotte (découplé des IDs Discord).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Bot {
    /// Nori-IA — roleplay nain (famille mistral).
    Noria,
    /// Winston — SOC tech (famille qwen).
    Winston,
    /// Wall-AI — alerting N1 (charge légère).
    WallAi,
}

/// Tier de modèle pour une requête.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tier {
    /// Modèle par défaut, full GPU, rapide.
    Default,
    /// Modèle d'escalade : plus gros, RAM-offload, lent mais exact.
    Escalation,
}

/// Requête d'inférence envoyée par un bot à bot-root.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceRequest {
    /// Bot émetteur (détermine la table de modèles).
    pub bot: Bot,
    /// Canal Discord (pour le turn-taking floor-control).
    pub channel: u64,
    /// Prompt système (peut être vide).
    #[serde(default)]
    pub system: String,
    /// Message utilisateur.
    pub user: String,
    /// Vrai si une pièce jointe à analyser accompagne la requête (signal d'escalade).
    #[serde(default)]
    pub has_attachment: bool,
    /// Override admin (`/root-adm-ollama`) : force un modèle et court-circuite le
    /// routeur de complexité. `None` = routage automatique.
    #[serde(default)]
    pub forced_model: Option<String>,
}

/// Réponse de bot-root à une requête d'inférence.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum InferenceResponse {
    /// Génération réussie.
    Ok {
        /// Modèle effectivement utilisé.
        model: String,
        /// Vrai si la requête a été escaladée vers le modèle d'escalade.
        escalated: bool,
        /// Texte généré.
        text: String,
    },
    /// Échec (canal invalide, bot inconnu, erreur backend...).
    Error {
        /// Message d'erreur lisible.
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_roundtrip_with_defaults() {
        let json = r#"{"bot":"Winston","channel":42,"user":"salut"}"#;
        let req: InferenceRequest = serde_json::from_str(json).expect("désérialisation");
        assert_eq!(req.channel, 42);
        assert_eq!(req.user, "salut");
        assert!(req.system.is_empty());
        assert!(!req.has_attachment);
        assert!(req.forced_model.is_none());
    }

    #[test]
    fn response_ok_serializes_with_tag() {
        let resp = InferenceResponse::Ok {
            model: "qwen3:8b".to_owned(),
            escalated: false,
            text: "bonjour".to_owned(),
        };
        let v: serde_json::Value = serde_json::to_value(&resp).expect("sérialisation");
        assert_eq!(v["status"], "ok");
        assert_eq!(v["model"], "qwen3:8b");
        assert_eq!(v["text"], "bonjour");
    }

    #[test]
    fn response_error_roundtrip() {
        let resp = InferenceResponse::Error { message: "bot inconnu".to_owned() };
        let s = serde_json::to_string(&resp).expect("ser");
        let back: InferenceResponse = serde_json::from_str(&s).expect("de");
        match back {
            InferenceResponse::Error { message } => assert_eq!(message, "bot inconnu"),
            _ => panic!("variante inattendue"),
        }
    }

    #[test]
    fn bot_enum_serializes_by_name() {
        assert_eq!(serde_json::to_string(&Bot::WallAi).unwrap(), "\"WallAi\"");
    }
}
