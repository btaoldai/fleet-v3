//! orchestrator — assemble le cœur de bot-root.
//!
//! Pour chaque requête : (1) acquiert le **jeton de parole** du canal
//! (turn-taking, D12), (2) détermine le **modèle** — override admin forcé, sinon
//! routage par complexité + table par-bot (D3/D10), (3) génère via le
//! **slot-manager** (accès GPU sérialisé). Le jeton est tenu pendant toute la
//! génération et libéré à la fin.

use fleet_core::ChannelId;
use tracing::info;

use crate::complexity_router::{classify, EscalationConfig};
use crate::floor_control::FloorControl;
use crate::model_registry::{ModelRegistry, Tier};
use crate::protocol::{InferenceRequest, InferenceResponse};
use crate::slot_manager::{GpuSlotManager, InferenceBackend};

/// Orchestrateur central : un par daemon bot-root.
pub struct Orchestrator<B: InferenceBackend> {
    slot: GpuSlotManager<B>,
    floor: FloorControl,
    registry: ModelRegistry,
    escalation: EscalationConfig,
}

impl<B: InferenceBackend> Orchestrator<B> {
    /// Assemble l'orchestrateur. Le floor-control est interne (créé ici).
    pub fn new(
        slot: GpuSlotManager<B>,
        registry: ModelRegistry,
        escalation: EscalationConfig,
    ) -> Self {
        Self {
            slot,
            floor: FloorControl::new(),
            registry,
            escalation,
        }
    }

    /// Traite une requête d'inférence de bout en bout.
    pub async fn handle(&self, req: InferenceRequest) -> InferenceResponse {
        // 1. Canal valide ?
        let channel = match ChannelId::new(req.channel) {
            Ok(c) => c,
            Err(_) => {
                return InferenceResponse::Error {
                    message: "canal invalide (0 n'est pas un snowflake)".to_owned(),
                };
            }
        };

        // 2. Jeton de parole du canal : tenu jusqu'à la fin de la génération.
        let _floor = self.floor.acquire(channel).await;

        // 3. Choix du modèle : override admin forcé, sinon routage automatique.
        let (model, escalated) = match req.forced_model {
            Some(forced) => (forced, false),
            None => {
                let Some(models) = self.registry.models_for(req.bot) else {
                    return InferenceResponse::Error {
                        message: format!("bot inconnu : {:?}", req.bot),
                    };
                };
                let tier = classify(&req.user, req.has_attachment, &self.escalation);
                let escalated = tier == Tier::Escalation && models.can_escalate();
                (models.model_for(tier).to_owned(), escalated)
            }
        };

        info!(bot = ?req.bot, channel = req.channel, model = %model, escalated, "dispatch inférence");

        // 4. Génération sérialisée via le slot GPU.
        match self.slot.run(&model, &req.system, &req.user).await {
            Ok(text) => InferenceResponse::Ok { model, escalated, text },
            Err(e) => InferenceResponse::Error { message: e.to_string() },
        }
        // _floor libéré ici (drop) -> le canal redevient disponible.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_registry::Bot;
    use crate::slot_manager::BackendError;
    use std::time::Duration;

    use async_trait::async_trait;

    /// Backend de test : renvoie le nom du modèle qu'on lui a demandé,
    /// ce qui permet d'asserter quel modèle a été routé.
    struct EchoBackend;

    #[async_trait]
    impl InferenceBackend for EchoBackend {
        async fn generate(&self, model: &str, _s: &str, _u: &str) -> Result<String, BackendError> {
            Ok(model.to_owned())
        }
    }

    fn orchestrator() -> Orchestrator<EchoBackend> {
        Orchestrator::new(
            GpuSlotManager::new(EchoBackend, Duration::ZERO),
            ModelRegistry::fleet_defaults(Some("qwen2.5:14b".to_owned())),
            EscalationConfig::default(),
        )
    }

    fn req(bot: Bot, user: &str) -> InferenceRequest {
        InferenceRequest {
            bot,
            channel: 7,
            system: String::new(),
            user: user.to_owned(),
            has_attachment: false,
            forced_model: None,
        }
    }

    #[tokio::test]
    async fn default_tier_uses_default_model() {
        let resp = orchestrator().handle(req(Bot::Winston, "salut, ca va ?")).await;
        match resp {
            InferenceResponse::Ok { model, escalated, text } => {
                assert_eq!(model, "qwen3:8b");
                assert!(!escalated);
                assert_eq!(text, "qwen3:8b"); // EchoBackend renvoie le modèle
            }
            other => panic!("attendu Ok, obtenu {other:?}"),
        }
    }

    #[tokio::test]
    async fn escalation_keyword_uses_escalation_model() {
        let resp = orchestrator()
            .handle(req(Bot::Winston, "peux-tu auditer ce document scientifique"))
            .await;
        match resp {
            InferenceResponse::Ok { model, escalated, .. } => {
                assert_eq!(model, "qwen2.5:14b");
                assert!(escalated);
            }
            other => panic!("attendu Ok, obtenu {other:?}"),
        }
    }

    #[tokio::test]
    async fn wallai_never_escalates() {
        // Mot-clé d'escalade présent, mais Wall-AI n'a pas de modèle d'escalade.
        let resp = orchestrator()
            .handle(req(Bot::WallAi, "fais un audit architecture complet"))
            .await;
        match resp {
            InferenceResponse::Ok { model, escalated, .. } => {
                assert_eq!(model, "qwen2.5:3b");
                assert!(!escalated);
            }
            other => panic!("attendu Ok, obtenu {other:?}"),
        }
    }

    #[tokio::test]
    async fn forced_model_bypasses_router() {
        let mut request = req(Bot::Noria, "peux-tu auditer ce document"); // mot-clé escalade
        request.forced_model = Some("custom:1b".to_owned());
        let resp = orchestrator().handle(request).await;
        match resp {
            InferenceResponse::Ok { model, escalated, .. } => {
                assert_eq!(model, "custom:1b");
                assert!(!escalated);
            }
            other => panic!("attendu Ok, obtenu {other:?}"),
        }
    }

    #[tokio::test]
    async fn invalid_channel_errors() {
        let mut request = req(Bot::Winston, "salut");
        request.channel = 0;
        let resp = orchestrator().handle(request).await;
        assert!(matches!(resp, InferenceResponse::Error { .. }));
    }

    #[tokio::test]
    async fn unknown_bot_errors() {
        let orch = Orchestrator::new(
            GpuSlotManager::new(EchoBackend, Duration::ZERO),
            ModelRegistry::new(), // registre vide
            EscalationConfig::default(),
        );
        let resp = orch.handle(req(Bot::Winston, "salut")).await;
        assert!(matches!(resp, InferenceResponse::Error { .. }));
    }
}
