//! bot-root — daemon gardien du GPU de la flotte srv-bot v3.
//!
//! bot-root est le **seul composant autorisé à piloter le GPU** : tous les bots
//! (et l'embedding AnythingLLM) passent par lui. Il sérialise globalement
//! l'unique slot 6 GiB et coordonne les tours de parole inter-bots.
//!
//! Modules :
//! - [`slot_manager`] : sérialisation du slot GPU unique (single-flight) +
//!   cooldown de swap (décisions D3/D10).
//! - [`floor_control`] : jeton de parole par canal, turn-taking inter-bots (D12).
//! - [`model_registry`] : table modèle ↔ tier ↔ bot, escalade intra-famille (D10).
//! - [`complexity_router`] : routage par escalade heuristique (D3).
//! - [`ollama_backend`] : adaptateur Ollama réel derrière le trait backend.
//! - [`orchestrator`] : assemble le tout (floor + routage + slot).
//! - [`protocol`] : types de requête/réponse du transport IPC (D8).
//!
//! À venir : serveur socket Unix + binaire daemon.
//!
//! Réf. architecture : `srv-bot/swarm/ARCHITECTURE-fleet-v3.md` (§4.3-4.7).

pub mod complexity_router;
pub mod floor_control;
pub mod model_registry;
pub mod ollama_backend;
pub mod orchestrator;
pub mod protocol;
pub mod server;
pub mod slot_manager;

pub use complexity_router::{classify, EscalationConfig};
pub use floor_control::{FloorControl, FloorGuard};
pub use model_registry::{Bot, BotModels, ModelRegistry, Tier};
pub use ollama_backend::OllamaBackend;
pub use orchestrator::Orchestrator;
pub use protocol::{InferenceRequest, InferenceResponse};
pub use slot_manager::{BackendError, GpuSlotManager, InferenceBackend, SlotError};
