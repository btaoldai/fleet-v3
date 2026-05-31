//! Binaire daemon bot-root — gardien du GPU de la flotte srv-bot v3.
//!
//! Configuration par variables d'environnement (valeurs par défaut entre
//! parenthèses) :
//! - `BOT_ROOT_SOCKET` (`/tmp/bot-root.sock`) : chemin du socket Unix IPC.
//! - `OLLAMA_URL` (`http://ollama:11434`) : URL d'Ollama.
//! - `OLLAMA_TIMEOUT_SECS` (`240`) : timeout HTTP (généreux pour le RAM-offload).
//! - `BOT_ROOT_SWAP_COOLDOWN_SECS` (`2`) : cooldown de settling au swap de modèle.
//! - `WINSTON_ESCALATION_MODEL` (absent) : modèle d'escalade Winston si tiré
//!   (ex. `qwen2.5:14b`) ; absent => Winston plafonné à `qwen3:8b`.

use std::sync::Arc;
use std::time::Duration;

use bot_root::server::serve;
use bot_root::{EscalationConfig, GpuSlotManager, ModelRegistry, OllamaBackend, Orchestrator};
use fleet_ollama::{OllamaClient, OllamaConfig};
use tokio::net::UnixListener;
use tracing::{info, warn};

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_owned())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let socket_path = env_or("BOT_ROOT_SOCKET", "/tmp/bot-root.sock");
    let ollama_url = env_or("OLLAMA_URL", "http://ollama:11434");
    let winston_escalation = std::env::var("WINSTON_ESCALATION_MODEL").ok();
    let cooldown_secs: u64 = env_or("BOT_ROOT_SWAP_COOLDOWN_SECS", "2").parse().unwrap_or(2);
    let timeout_secs: u64 = env_or("OLLAMA_TIMEOUT_SECS", "240").parse().unwrap_or(240);

    let client = OllamaClient::new(
        OllamaConfig::new(ollama_url, "qwen3:8b")
            .timeout_secs(timeout_secs)
            .temperature(0.3),
    )?;
    let slot = GpuSlotManager::new(OllamaBackend::new(client), Duration::from_secs(cooldown_secs));
    let registry = ModelRegistry::fleet_defaults(winston_escalation);
    let orchestrator = Arc::new(Orchestrator::new(slot, registry, EscalationConfig::default()));

    // Retire un éventuel socket résiduel avant de (re)binder.
    let _ = std::fs::remove_file(&socket_path);
    let listener = UnixListener::bind(&socket_path)?;
    info!(socket = %socket_path, "bot-root en écoute (gardien GPU)");

    tokio::select! {
        result = serve(listener, orchestrator) => {
            if let Err(e) = result {
                warn!(error = %e, "serveur arrêté sur erreur");
            }
        }
        _ = tokio::signal::ctrl_c() => {
            info!("signal d'arrêt reçu, fermeture propre");
        }
    }

    let _ = std::fs::remove_file(&socket_path);
    Ok(())
}
