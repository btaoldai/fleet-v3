//! gpu-slot-manager — sérialise tout accès au GPU (slot modèle unique).
//!
//! Principe : un seul modèle réside en VRAM à la fois (contrainte 6 GiB). Le
//! manager tient un verrou pendant toute la génération — **single-flight** : une
//! seule requête touche le GPU à la fois. Au changement de modèle (swap), il
//! applique un **cooldown de settling** avant de générer, ce qui empêche le
//! swap-thrash qui a fait échouer le test 50 tours (T87).
//!
//! Le manager ne pilote pas l'offload GPU/RAM : c'est Ollama qui le gère
//! automatiquement (décision D10). Le manager choisit le modèle et sérialise.

use std::time::Duration;

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{debug, warn};

/// Erreur renvoyée par un backend d'inférence.
#[derive(Debug, Error)]
#[error("échec du backend d'inférence : {0}")]
pub struct BackendError(pub String);

/// Erreur du slot-manager.
#[derive(Debug, Error)]
pub enum SlotError {
    /// Le backend d'inférence a échoué.
    #[error(transparent)]
    Backend(#[from] BackendError),
}

/// Backend capable de générer une réponse pour un modèle donné.
///
/// Abstrait Ollama derrière un trait pour permettre les tests sans GPU.
#[async_trait]
pub trait InferenceBackend: Send + Sync {
    /// Génère une réponse. `model` est le nom Ollama (ex. `qwen3:8b`).
    async fn generate(&self, model: &str, system: &str, user: &str)
        -> Result<String, BackendError>;
}

#[derive(Debug, Default)]
struct SlotState {
    /// Modèle actuellement résident (`None` au démarrage).
    current_model: Option<String>,
    /// Nombre de swaps effectifs (transitions entre deux modèles distincts).
    swaps: u64,
}

/// Gardien du slot GPU unique.
///
/// Sérialise les requêtes (single-flight) et applique un cooldown au changement
/// de modèle. Générique sur le backend pour la testabilité.
pub struct GpuSlotManager<B: InferenceBackend> {
    backend: B,
    state: Mutex<SlotState>,
    cooldown: Duration,
}

impl<B: InferenceBackend> GpuSlotManager<B> {
    /// Crée le manager avec un `cooldown` de settling appliqué à chaque swap.
    /// `Duration::ZERO` désactive le cooldown (utile en test).
    pub fn new(backend: B, cooldown: Duration) -> Self {
        Self {
            backend,
            state: Mutex::new(SlotState::default()),
            cooldown,
        }
    }

    /// Exécute une requête d'inférence en sérialisation stricte.
    ///
    /// Tient le verrou du slot pendant toute la génération : un seul accès GPU à
    /// la fois. Au swap (modèle différent du résident), incrémente le compteur et
    /// applique le cooldown avant de générer.
    pub async fn run(&self, model: &str, system: &str, user: &str) -> Result<String, SlotError> {
        let mut state = self.state.lock().await;

        let needs_load = state.current_model.as_deref() != Some(model);
        let is_swap = needs_load && state.current_model.is_some();
        if is_swap {
            state.swaps += 1;
            warn!(from = ?state.current_model, to = model, "swap de modèle GPU");
            if !self.cooldown.is_zero() {
                tokio::time::sleep(self.cooldown).await;
            }
        }

        debug!(model, "génération (slot verrouillé)");
        let out = self.backend.generate(model, system, user).await?;
        state.current_model = Some(model.to_owned());
        Ok(out)
    }

    /// Modèle actuellement résident.
    pub async fn current_model(&self) -> Option<String> {
        self.state.lock().await.current_model.clone()
    }

    /// Nombre de swaps effectués depuis le démarrage.
    pub async fn swap_count(&self) -> u64 {
        self.state.lock().await.swaps
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[derive(Default)]
    struct Counters {
        in_flight: AtomicUsize,
        max_in_flight: AtomicUsize,
        calls: AtomicUsize,
    }

    struct MockBackend {
        counters: Arc<Counters>,
    }

    #[async_trait]
    impl InferenceBackend for MockBackend {
        async fn generate(&self, model: &str, _s: &str, _u: &str) -> Result<String, BackendError> {
            let now = self.counters.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            self.counters.max_in_flight.fetch_max(now, Ordering::SeqCst);
            // Laisse une fenêtre d'entrelacement : si la sérialisation était
            // cassée, une autre tâche entrerait ici en parallèle.
            tokio::task::yield_now().await;
            self.counters.calls.fetch_add(1, Ordering::SeqCst);
            self.counters.in_flight.fetch_sub(1, Ordering::SeqCst);
            Ok(format!("réponse de {model}"))
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn serializes_single_flight() {
        let counters = Arc::new(Counters::default());
        let mgr = Arc::new(GpuSlotManager::new(
            MockBackend { counters: Arc::clone(&counters) },
            Duration::ZERO,
        ));

        let mut handles = Vec::new();
        for i in 0..16 {
            let m = Arc::clone(&mgr);
            handles.push(tokio::spawn(async move {
                m.run("qwen3:8b", "", &format!("msg {i}")).await.expect("run");
            }));
        }
        for h in handles {
            h.await.expect("join");
        }

        assert_eq!(
            counters.max_in_flight.load(Ordering::SeqCst),
            1,
            "un seul accès GPU à la fois (single-flight)"
        );
        assert_eq!(counters.calls.load(Ordering::SeqCst), 16);
    }

    #[tokio::test]
    async fn counts_swaps_and_tracks_model() {
        let counters = Arc::new(Counters::default());
        let mgr = GpuSlotManager::new(
            MockBackend { counters: Arc::clone(&counters) },
            Duration::ZERO,
        );

        mgr.run("a", "", "x").await.unwrap(); // None -> a : chargement, pas un swap
        mgr.run("a", "", "x").await.unwrap(); // a -> a : rien
        mgr.run("b", "", "x").await.unwrap(); // a -> b : swap
        mgr.run("a", "", "x").await.unwrap(); // b -> a : swap

        assert_eq!(mgr.swap_count().await, 2, "deux swaps effectifs");
        assert_eq!(mgr.current_model().await.as_deref(), Some("a"));
        assert_eq!(counters.calls.load(Ordering::SeqCst), 4);
    }

    #[tokio::test]
    async fn propagates_backend_error() {
        struct Failing;
        #[async_trait]
        impl InferenceBackend for Failing {
            async fn generate(&self, _m: &str, _s: &str, _u: &str) -> Result<String, BackendError> {
                Err(BackendError("boom".to_owned()))
            }
        }
        let mgr = GpuSlotManager::new(Failing, Duration::ZERO);
        let err = mgr.run("a", "", "x").await.unwrap_err();
        assert!(matches!(err, SlotError::Backend(_)));
    }
}
