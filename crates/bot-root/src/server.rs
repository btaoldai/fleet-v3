//! server — serveur IPC socket Unix (D8).
//!
//! Protocole : une requête JSON [`InferenceRequest`] par ligne ; bot-root répond
//! par une ligne JSON [`InferenceResponse`]. Une connexion peut enchaîner
//! plusieurs requêtes (keep-alive). Chaque connexion est traitée dans sa propre
//! tâche ; la sérialisation GPU est assurée en aval par le slot-manager.

use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tracing::{debug, warn};

use crate::orchestrator::Orchestrator;
use crate::protocol::{InferenceRequest, InferenceResponse};
use crate::slot_manager::InferenceBackend;

/// Boucle d'acceptation : sert les connexions jusqu'à erreur fatale du listener.
pub async fn serve<B>(
    listener: UnixListener,
    orchestrator: Arc<Orchestrator<B>>,
) -> std::io::Result<()>
where
    B: InferenceBackend + 'static,
{
    loop {
        let (stream, _addr) = listener.accept().await?;
        let orch = Arc::clone(&orchestrator);
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, orch).await {
                warn!(error = %e, "connexion terminée sur erreur");
            }
        });
    }
}

/// Traite une connexion : lit des requêtes ligne-à-ligne et répond à chacune.
async fn handle_connection<B>(
    stream: UnixStream,
    orchestrator: Arc<Orchestrator<B>>,
) -> std::io::Result<()>
where
    B: InferenceBackend,
{
    let (read_half, mut write_half) = stream.into_split();
    let mut lines = BufReader::new(read_half).lines();

    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<InferenceRequest>(&line) {
            Ok(req) => {
                debug!("requête reçue");
                orchestrator.handle(req).await
            }
            Err(e) => InferenceResponse::Error {
                message: format!("requête JSON invalide : {e}"),
            },
        };

        let mut payload = serde_json::to_string(&response).unwrap_or_else(|_| {
            r#"{"status":"error","message":"échec de sérialisation de la réponse"}"#.to_owned()
        });
        payload.push('\n');
        write_half.write_all(payload.as_bytes()).await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use async_trait::async_trait;

    use crate::complexity_router::EscalationConfig;
    use crate::model_registry::ModelRegistry;
    use crate::slot_manager::{BackendError, GpuSlotManager};

    struct EchoBackend;

    #[async_trait]
    impl InferenceBackend for EchoBackend {
        async fn generate(&self, model: &str, _s: &str, _u: &str) -> Result<String, BackendError> {
            Ok(format!("echo:{model}"))
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn socket_request_response_roundtrip() {
        // Socket dans /tmp (FS Linux natif) — pas sur /mnt/c qui ne supporte pas
        // les sockets Unix.
        let path = std::env::temp_dir().join(format!("bot-root-test-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).expect("bind socket");

        let orchestrator = Arc::new(Orchestrator::new(
            GpuSlotManager::new(EchoBackend, Duration::ZERO),
            ModelRegistry::fleet_defaults(Some("qwen2.5:14b".to_owned())),
            EscalationConfig::default(),
        ));
        let server = tokio::spawn(serve(listener, orchestrator));

        // Client : envoie une requête, lit la réponse.
        let stream = UnixStream::connect(&path).await.expect("connexion client");
        let (read_half, mut write_half) = stream.into_split();
        write_half
            .write_all(b"{\"bot\":\"Winston\",\"channel\":7,\"user\":\"salut\"}\n")
            .await
            .expect("écriture requête");

        let mut lines = BufReader::new(read_half).lines();
        let line = lines
            .next_line()
            .await
            .expect("lecture réponse")
            .expect("une ligne de réponse");

        let response: InferenceResponse = serde_json::from_str(&line).expect("parse réponse");
        match response {
            InferenceResponse::Ok { model, text, .. } => {
                assert_eq!(model, "qwen3:8b");
                assert_eq!(text, "echo:qwen3:8b");
            }
            other => panic!("attendu Ok, obtenu {other:?}"),
        }

        server.abort();
        let _ = std::fs::remove_file(&path);
    }
}
