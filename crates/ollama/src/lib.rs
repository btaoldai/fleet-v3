//! fleet-ollama — client Ollama unifié de la flotte srv-bot v3.
//!
//! Résorbe les **trois signatures divergentes** des bots (dette n°1 de l'audit
//! 2026-05-31) en un client unique configuré par injection ([`OllamaConfig`]) :
//!
//! - Nori-IA : `new(base_url, timeout_secs)`
//! - Winston : `new(base_url, timeout_secs, default_model, temperature)`
//! - Wall-AI : `new(base_url, model, owner_discord_id)`
//!
//! Unification : un seul [`OllamaClient::new`] prenant une [`OllamaConfig`]
//! (base_url + default_model + timeout + temperature + keep_alive + owner).
//!
//! Correctif au passage : `keep_alive` est envoyé au **niveau racine** de
//! `/api/chat` (et non dans `options`, où Nori-IA le plaçait à tort — l'API
//! Ollama ne l'y lit pas).
//!
//! Réf. architecture : `srv-bot/swarm/ARCHITECTURE-fleet-v3.md` (§4.2, D4).

mod client;
mod config;
pub mod error;

pub use client::OllamaClient;
pub use config::OllamaConfig;
pub use error::{OllamaError, Result};
