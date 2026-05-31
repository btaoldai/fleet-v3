//! protocol — ré-export des types de fil partagés.
//!
//! Les types vivent désormais dans la crate `fleet-protocol` (partagée avec les
//! clients). Ce module les ré-expose pour préserver les chemins `crate::protocol::*`
//! internes à bot-root.

pub use fleet_protocol::{InferenceRequest, InferenceResponse};
