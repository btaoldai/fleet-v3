//! fleet-core — socle partagé de la flotte srv-bot (architecture v3).
//!
//! Briques transverses, sans dépendance à Discord ni à Ollama :
//! - [`error`] : type d'erreur unifié [`CoreError`] et alias [`Result`].
//! - [`secrets`] : lecture de secrets façon Docker (`*_FILE`) avec repli env.
//! - [`ids`] : identifiants Discord validés (newtypes non-nuls, type-safe).
//!
//! Réf. architecture : `srv-bot/swarm/ARCHITECTURE-fleet-v3.md` (décision D4 —
//! uniformisation par crates partagées).

pub mod error;
pub mod ids;
pub mod secrets;

pub use error::{CoreError, Result};
pub use ids::{BotId, ChannelId, UserId};
pub use secrets::read_secret_or_env;
