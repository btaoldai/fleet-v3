//! Exemple minimal : envoyer une requête d'inférence au daemon bot-root.
//!
//! Prérequis : le daemon `bot-root` tourne (`cargo run --bin bot-root`) et Ollama
//! est joignable avec le modèle par défaut tiré (`ollama pull qwen2.5:3b`).
//!
//! Usage :
//! ```bash
//! cargo run -p bot-root-client --example send_request
//! # ou avec un socket personnalisé :
//! BOT_ROOT_SOCKET=/tmp/bot-root.sock cargo run -p bot-root-client --example send_request
//! ```

use bot_root_client::BotRootClient;
use fleet_protocol::{Bot, InferenceRequest};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let socket =
        std::env::var("BOT_ROOT_SOCKET").unwrap_or_else(|_| "/tmp/bot-root.sock".to_owned());
    let client = BotRootClient::new(socket);

    let request = InferenceRequest {
        bot: Bot::WallAi,
        channel: 1,
        system: "Tu es un assistant concis.".to_owned(),
        user: "Dis bonjour en une phrase.".to_owned(),
        has_attachment: false,
        forced_model: None,
    };

    println!("-> requête envoyée à bot-root (bot = WallAi)...");
    let response = client.infer(&request).await?;
    println!("<- réponse : {response:#?}");
    Ok(())
}
