//! fleet-personas — mécanisme partagé de routage de personas.
//!
//! Généralisation du module `personas/` de Wall-AI (le plus mature de la flotte,
//! cf. audit 2026-05-31). La nouveauté : le **registre de personas n'est plus
//! câblé** dans la crate — il est fourni par chaque bot. Seul le **mécanisme**
//! (type [`Persona`], routeur [`select_personas`], constructeur
//! [`build_system_prompt`]) est mutualisé, pour rester réutilisable selon la
//! persona et la mission de chaque bot (décision D11 — capacités composables).
//!
//! # Exemple
//! ```
//! use fleet_personas::{Persona, RouterConfig, select_personas, build_system_prompt};
//!
//! static SECU: Persona = Persona {
//!     id: "secu", name: "Secu", keywords: &["audit", "wazuh"],
//!     system_prompt: "## Persona Secu",
//! };
//! static REGISTRY: &[&Persona] = &[&SECU];
//!
//! let selected = select_personas("audit wazuh", REGISTRY, &RouterConfig::default());
//! assert!(selected.iter().any(|p| p.id == "secu"));
//!
//! let prompt = build_system_prompt("Tu es un bot.", &selected, "Alice", 42);
//! assert!(prompt.contains("Alice"));
//! ```
//!
//! Réf. architecture : `srv-bot/swarm/ARCHITECTURE-fleet-v3.md` (§2.2, D4).

mod prompt_builder;
mod router;

pub use prompt_builder::{build_system_prompt, with_datetime_context};
pub use router::{select_personas, RouterConfig};

/// Représente un skill-persona pouvant enrichir le system prompt d'Ollama.
///
/// Les instances sont statiques (`'static`) — déclarées une fois dans le registre
/// d'un bot et partagées par référence pendant toute la session.
#[derive(Debug)]
pub struct Persona {
    /// Identifiant technique court (ex. `"redblue-senior"`).
    pub id: &'static str,
    /// Nom affiché (ex. `"RedBlue Senior"`).
    pub name: &'static str,
    /// Mots-clés déclencheurs. Correspondance insensible à la casse, par
    /// sous-chaîne.
    pub keywords: &'static [&'static str],
    /// System prompt condensé injecté dans Ollama quand le persona est actif.
    pub system_prompt: &'static str,
}

#[cfg(test)]
pub(crate) mod fixtures {
    //! Petit registre de test — remplace le contenu Wall-AI pour des tests
    //! autonomes du mécanisme.
    use super::Persona;

    pub static SECU: Persona = Persona {
        id: "secu",
        name: "Secu Senior",
        keywords: &["audit", "wazuh", "pentest", "soc"],
        system_prompt: "## Persona actif : Secu Senior",
    };
    pub static RUST: Persona = Persona {
        id: "rust",
        name: "Rust Expert",
        keywords: &["trait", "async", "rust", "cargo"],
        system_prompt: "## Persona actif : Rust Expert",
    };
    pub static COACH: Persona = Persona {
        id: "coach",
        name: "Prompt Coach",
        keywords: &[],
        system_prompt: "## Persona actif : Prompt Coach",
    };

    pub static REGISTRY: &[&Persona] = &[&SECU, &RUST, &COACH];
}
