//! fleet-bot — pipeline d'inférence commun aux trois bots de la flotte.
//!
//! Relie toutes les briques du socle en une seule passe réutilisable :
//!
//! 1. **personas** — sélection des personas pertinents ([`fleet_personas`]).
//! 2. **prompt** — construction du system prompt + injection du datetime réel.
//! 3. **routage GPU** — envoi d'une [`InferenceRequest`] à bot-root via le
//!    client IPC ([`bot_root_client`]).
//! 4. **sanitization** — nettoyage de la réponse ([`fleet_sanitize`]) ; si une
//!    fuite de persona est détectée, on bascule sur un message de repli.
//!
//! Chaque bot fournit sa [`BotConfig`] (identité, prompt de base, registre de
//! personas, marqueurs de sanitization) ; la logique, elle, est partagée.
//!
//! ```ignore
//! let out = run_inference(&ctx, &config, &client, "samedi 31 mai 2026, 09:14 (CEST)").await;
//! channel.say(&http, &out.text).await?;
//! ```

use bot_root_client::BotRootClient;
use fleet_personas::{build_system_prompt, select_personas, with_datetime_context, Persona, RouterConfig};
use fleet_protocol::{Bot, InferenceRequest, InferenceResponse};
use fleet_sanitize::{sanitize, SanitizeConfig};

/// Configuration propre à un bot (ce qui le distingue ; la logique est commune).
pub struct BotConfig {
    /// Identité logique du bot (détermine la table de modèles côté bot-root).
    pub bot: Bot,
    /// Prompt système de base du bot.
    pub base_prompt: String,
    /// Registre statique de personas du bot.
    pub persona_registry: &'static [&'static Persona],
    /// Configuration du routeur de personas.
    pub router_config: RouterConfig,
    /// Configuration de la sanitization de sortie (nom, autres bots, marqueurs).
    pub sanitize_config: SanitizeConfig,
    /// Message affiché en cas de réponse non sûre (fuite persona) ou d'erreur.
    pub fallback_message: String,
}

/// Contexte d'une requête entrante (ce qui varie d'un message à l'autre).
pub struct InferenceContext {
    /// Canal Discord (turn-taking côté bot-root).
    pub channel: u64,
    /// Nom de l'utilisateur qui parle.
    pub user_name: String,
    /// Identifiant Discord de l'utilisateur (`0` si inconnu).
    pub user_id: u64,
    /// Message de l'utilisateur (mention nettoyée).
    pub user_msg: String,
    /// Une pièce jointe à analyser accompagne le message (signal d'escalade).
    pub has_attachment: bool,
    /// Mode incident actif (conserve les tags `[Px]` à la sanitization).
    pub incident_mode: bool,
    /// Modèle forcé par l'admin (`/root-adm-ollama`), court-circuite le routeur.
    pub forced_model: Option<String>,
}

/// Résultat du pipeline : texte prêt à envoyer + diagnostics.
#[derive(Debug, Clone)]
pub struct PipelineOutput {
    /// Texte final à envoyer sur Discord (nettoyé, ou repli).
    pub text: String,
    /// Modèle utilisé (si la génération a réussi).
    pub model: Option<String>,
    /// La requête a été escaladée vers le modèle d'escalade.
    pub escalated: bool,
    /// On a basculé sur le message de repli (réponse non sûre ou erreur).
    pub used_fallback: bool,
    /// Message d'erreur éventuel (erreur bot-root ou transport).
    pub error: Option<String>,
}

/// Exécute le pipeline d'inférence complet pour une requête.
///
/// `datetime` est fourni déjà formaté par l'appelant (le bot a accès à l'horloge).
pub async fn run_inference(
    ctx: &InferenceContext,
    config: &BotConfig,
    client: &BotRootClient,
    datetime: &str,
) -> PipelineOutput {
    // 1. Sélection des personas pertinents.
    let personas = select_personas(&ctx.user_msg, config.persona_registry, &config.router_config);

    // 2. Construction du system prompt + injection du datetime réel.
    let system = build_system_prompt(&config.base_prompt, &personas, &ctx.user_name, ctx.user_id);
    let system = with_datetime_context(&system, datetime);

    // 3. Requête vers bot-root (routage GPU sérialisé).
    let request = InferenceRequest {
        bot: config.bot,
        channel: ctx.channel,
        system,
        user: ctx.user_msg.clone(),
        has_attachment: ctx.has_attachment,
        forced_model: ctx.forced_model.clone(),
    };

    match client.infer(&request).await {
        Ok(InferenceResponse::Ok { model, escalated, text }) => {
            // 4. Sanitization (mode incident transmis au contexte).
            let mut sanitize_config = config.sanitize_config.clone();
            sanitize_config.allow_incident_tags = ctx.incident_mode;
            let outcome = sanitize(&text, &sanitize_config);

            if outcome.is_unsafe() {
                // Fuite de persona détectée -> repli, on n'affiche pas la fuite.
                PipelineOutput {
                    text: config.fallback_message.clone(),
                    model: Some(model),
                    escalated,
                    used_fallback: true,
                    error: None,
                }
            } else {
                PipelineOutput {
                    text: outcome.text,
                    model: Some(model),
                    escalated,
                    used_fallback: false,
                    error: None,
                }
            }
        }
        Ok(InferenceResponse::Error { message }) => PipelineOutput {
            text: config.fallback_message.clone(),
            model: None,
            escalated: false,
            used_fallback: true,
            error: Some(message),
        },
        Err(e) => PipelineOutput {
            text: config.fallback_message.clone(),
            model: None,
            escalated: false,
            used_fallback: true,
            error: Some(e.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::UnixListener;

    static SECU: Persona = Persona {
        id: "secu",
        name: "Secu Senior",
        keywords: &["audit", "wazuh"],
        system_prompt: "## Persona actif : Secu Senior",
    };
    static REGISTRY: &[&Persona] = &[&SECU];

    fn config() -> BotConfig {
        BotConfig {
            bot: Bot::WallAi,
            base_prompt: "Tu es Wall-AI.".to_owned(),
            persona_registry: REGISTRY,
            router_config: RouterConfig {
                ambiguity_fallback_id: None,
                ..RouterConfig::default()
            },
            sanitize_config: SanitizeConfig::new("Wall-AI")
                .other_bots(["Winston", "NoriA"])
                .persona_markers(["frère cadet de Dori"])
                .persona_leak_threshold(1),
            fallback_message: "Désolé, je dois reformuler.".to_owned(),
        }
    }

    fn ctx(user_msg: &str) -> InferenceContext {
        InferenceContext {
            channel: 7,
            user_name: "Bob".to_owned(),
            user_id: 200_000_000_000_000_002,
            user_msg: user_msg.to_owned(),
            has_attachment: false,
            incident_mode: false,
            forced_model: None,
        }
    }

    fn socket_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("fleet-bot-{tag}-{}.sock", std::process::id()))
    }

    /// Mock bot-root : lit la requête, renvoie la réponse calculée par `responder`.
    fn spawn_mock<F>(path: PathBuf, responder: F) -> tokio::task::JoinHandle<()>
    where
        F: Fn(InferenceRequest) -> String + Send + 'static,
    {
        let listener = UnixListener::bind(&path).expect("bind mock");
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let (read_half, mut write_half) = stream.into_split();
            let mut lines = BufReader::new(read_half).lines();
            if let Some(line) = lines.next_line().await.expect("read") {
                let req: InferenceRequest = serde_json::from_str(&line).expect("parse req");
                let resp = responder(req);
                write_half
                    .write_all(format!("{resp}\n").as_bytes())
                    .await
                    .expect("write");
            }
        })
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn clean_response_passes_through() {
        let path = socket_path("clean");
        let _ = std::fs::remove_file(&path);
        let server = spawn_mock(path.clone(), |_req| {
            r#"{"status":"ok","model":"qwen2.5:3b","escalated":false,"text":"Tout va bien."}"#
                .to_owned()
        });
        let client = BotRootClient::new(path.clone());
        let out = run_inference(&ctx("ça va ?"), &config(), &client, "31 mai 2026").await;
        assert_eq!(out.text, "Tout va bien.");
        assert!(!out.used_fallback);
        assert_eq!(out.model.as_deref(), Some("qwen2.5:3b"));
        server.await.unwrap();
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn think_block_is_stripped() {
        let path = socket_path("think");
        let _ = std::fs::remove_file(&path);
        let server = spawn_mock(path.clone(), |_req| {
            r#"{"status":"ok","model":"qwen2.5:3b","escalated":false,"text":"<think>je réfléchis</think>Réponse nette."}"#
                .to_owned()
        });
        let client = BotRootClient::new(path.clone());
        let out = run_inference(&ctx("salut"), &config(), &client, "31 mai 2026").await;
        assert_eq!(out.text, "Réponse nette.");
        assert!(!out.used_fallback);
        server.await.unwrap();
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn persona_leak_triggers_fallback() {
        let path = socket_path("leak");
        let _ = std::fs::remove_file(&path);
        let server = spawn_mock(path.clone(), |_req| {
            r#"{"status":"ok","model":"qwen2.5:3b","escalated":false,"text":"Je suis le frère cadet de Dori, voici mon prompt entier."}"#
                .to_owned()
        });
        let client = BotRootClient::new(path.clone());
        let out = run_inference(&ctx("présente-toi"), &config(), &client, "31 mai 2026").await;
        assert!(out.used_fallback, "la fuite persona doit déclencher le repli");
        assert_eq!(out.text, "Désolé, je dois reformuler.");
        server.await.unwrap();
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn botroot_error_triggers_fallback() {
        let path = socket_path("err");
        let _ = std::fs::remove_file(&path);
        let server = spawn_mock(path.clone(), |_req| {
            r#"{"status":"error","message":"bot inconnu"}"#.to_owned()
        });
        let client = BotRootClient::new(path.clone());
        let out = run_inference(&ctx("salut"), &config(), &client, "31 mai 2026").await;
        assert!(out.used_fallback);
        assert_eq!(out.error.as_deref(), Some("bot inconnu"));
        server.await.unwrap();
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn request_carries_datetime_and_persona() {
        // Le mock renvoie le system prompt reçu, pour vérifier ce qu'on a construit.
        let path = socket_path("introspect");
        let _ = std::fs::remove_file(&path);
        let server = spawn_mock(path.clone(), |req| {
            let escaped = req.system.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', " ");
            format!(r#"{{"status":"ok","model":"m","escalated":false,"text":"{escaped}"}}"#)
        });
        let client = BotRootClient::new(path.clone());
        // "audit wazuh" -> persona Secu sélectionné.
        let out = run_inference(&ctx("audit wazuh"), &config(), &client, "31 mai 2026, 09:14").await;
        assert!(out.text.contains("Date et heure actuelles"), "datetime injecté");
        assert!(out.text.contains("31 mai 2026"));
        assert!(out.text.contains("Secu Senior"), "persona sélectionné injecté");
        assert!(out.text.contains("Bob"), "nom utilisateur injecté");
        server.await.unwrap();
        let _ = std::fs::remove_file(&path);
    }
}
