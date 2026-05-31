//! Erreurs typées du client Ollama (thiserror, doctrine baptiste-code-style).

use thiserror::Error;

/// Erreurs du client Ollama.
#[derive(Debug, Error)]
pub enum OllamaError {
    /// Échec de construction du client HTTP sous-jacent.
    #[error("échec de construction du client HTTP")]
    Build(#[source] reqwest::Error),

    /// La requête réseau vers Ollama a échoué (timeout, connexion, transport).
    #[error("requête Ollama échouée vers {url}")]
    Request {
        /// URL ciblée.
        url: String,
        /// Cause réseau sous-jacente.
        #[source]
        source: reqwest::Error,
    },

    /// Ollama a répondu avec un statut HTTP non-2xx.
    #[error("Ollama a renvoyé le statut HTTP {status} : {body}")]
    Http {
        /// Code de statut HTTP.
        status: u16,
        /// Corps de la réponse (tronqué si nécessaire par le serveur).
        body: String,
    },

    /// La réponse d'Ollama n'a pas pu être désérialisée (JSON inattendu).
    #[error("réponse Ollama illisible (JSON)")]
    Decode(#[source] reqwest::Error),
}

/// Alias de résultat du client Ollama.
pub type Result<T> = std::result::Result<T, OllamaError>;
