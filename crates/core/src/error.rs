//! Type d'erreur unifié du socle.
//!
//! Doctrine baptiste-code-style : erreurs typées via `thiserror` côté
//! bibliothèque, jamais d'`unwrap()` en production.

use thiserror::Error;

/// Erreurs du socle partagé.
#[derive(Debug, Error)]
pub enum CoreError {
    /// Un identifiant Discord nul a été fourni (0 n'est pas un snowflake valide).
    #[error("identifiant Discord invalide : 0 n'est pas un snowflake valide")]
    InvalidDiscordId,

    /// Un secret obligatoire est absent (ni fichier `*_FILE`, ni variable d'env).
    #[error("secret manquant : ni `{file_env}` ni `{value_env}` ne sont utilisables")]
    MissingSecret {
        /// Variable pointant vers le fichier secret (ex. `DISCORD_TOKEN_FILE`).
        file_env: String,
        /// Variable portant la valeur directe (ex. `DISCORD_TOKEN`).
        value_env: String,
    },
}

/// Alias de résultat du socle.
pub type Result<T> = std::result::Result<T, CoreError>;
