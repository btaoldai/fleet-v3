//! bot-root-client — client IPC léger pour parler au daemon bot-root.
//!
//! Un bot construit un [`BotRootClient`] avec le chemin du socket Unix, puis
//! appelle [`BotRootClient::infer`] pour chaque requête. Dépend uniquement de
//! `fleet-protocol` + tokio (pas du daemon entier).
//!
//! Protocole (D8) : une requête JSON par ligne, une réponse JSON par ligne.

use std::path::PathBuf;

use fleet_protocol::{InferenceRequest, InferenceResponse};
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

/// Erreurs du client bot-root.
#[derive(Debug, Error)]
pub enum ClientError {
    /// Connexion au socket impossible (daemon absent, permissions...).
    #[error("connexion à bot-root échouée ({path})")]
    Connect {
        /// Chemin du socket visé.
        path: String,
        /// Cause E/S sous-jacente.
        #[source]
        source: std::io::Error,
    },
    /// Erreur d'E/S pendant l'échange.
    #[error("E/S avec bot-root")]
    Io(#[from] std::io::Error),
    /// Encodage de la requête en JSON impossible.
    #[error("encodage de la requête échoué")]
    Encode(#[source] serde_json::Error),
    /// Réponse illisible (JSON inattendu).
    #[error("réponse bot-root illisible (JSON)")]
    Decode(#[source] serde_json::Error),
    /// bot-root a fermé la connexion sans répondre.
    #[error("bot-root a fermé la connexion sans répondre")]
    NoResponse,
}

/// Client IPC vers bot-root. Bon marché à cloner (juste un chemin).
#[derive(Debug, Clone)]
pub struct BotRootClient {
    socket_path: PathBuf,
}

impl BotRootClient {
    /// Crée un client visant le socket donné.
    pub fn new(socket_path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
        }
    }

    /// Envoie une requête d'inférence et attend la réponse.
    ///
    /// Ouvre une connexion par appel (simple et robuste ; le daemon supporte le
    /// keep-alive si on veut optimiser plus tard).
    pub async fn infer(&self, request: &InferenceRequest) -> Result<InferenceResponse, ClientError> {
        let stream = UnixStream::connect(&self.socket_path)
            .await
            .map_err(|source| ClientError::Connect {
                path: self.socket_path.display().to_string(),
                source,
            })?;

        let (read_half, mut write_half) = stream.into_split();

        let mut line = serde_json::to_string(request).map_err(ClientError::Encode)?;
        line.push('\n');
        write_half.write_all(line.as_bytes()).await?;
        write_half.flush().await?;

        let mut lines = BufReader::new(read_half).lines();
        match lines.next_line().await? {
            Some(response_line) => {
                serde_json::from_str(&response_line).map_err(ClientError::Decode)
            }
            None => Err(ClientError::NoResponse),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fleet_protocol::Bot;
    use tokio::net::UnixListener;

    fn sample_request() -> InferenceRequest {
        InferenceRequest {
            bot: Bot::Winston,
            channel: 7,
            system: String::new(),
            user: "ping".to_owned(),
            has_attachment: false,
            forced_model: None,
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn infer_roundtrip_against_mock_server() {
        let path =
            std::env::temp_dir().join(format!("br-client-test-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).expect("bind socket mock");

        // Mock server : lit une requête, renvoie une réponse Ok canned.
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let (read_half, mut write_half) = stream.into_split();
            let mut lines = BufReader::new(read_half).lines();
            let _req = lines.next_line().await.expect("read").expect("une ligne");
            let resp = r#"{"status":"ok","model":"qwen3:8b","escalated":false,"text":"pong"}"#;
            write_half
                .write_all(format!("{resp}\n").as_bytes())
                .await
                .expect("write");
        });

        let client = BotRootClient::new(path.clone());
        let response = client.infer(&sample_request()).await.expect("infer");
        match response {
            InferenceResponse::Ok { model, text, .. } => {
                assert_eq!(model, "qwen3:8b");
                assert_eq!(text, "pong");
            }
            other => panic!("attendu Ok, obtenu {other:?}"),
        }

        server.await.expect("join serveur mock");
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn connect_error_when_no_server() {
        let client = BotRootClient::new("/tmp/bot-root-absent-xyz.sock");
        let err = client.infer(&sample_request()).await.unwrap_err();
        assert!(matches!(err, ClientError::Connect { .. }));
    }
}
