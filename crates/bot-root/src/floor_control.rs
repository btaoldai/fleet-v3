//! floor-control — jeton de parole par canal (turn-taking inter-bots).
//!
//! Problème (décision D12) : en conversation multi-bots, rien n'empêche un bot
//! de répondre pendant qu'un autre rédige encore -> overflood. bot-root détient
//! l'**état de vérité** : un seul détenteur de jeton par canal. Un bot acquiert
//! le jeton avant de rédiger ; tant qu'il le tient, les autres attendent. À la
//! libération (drop du [`FloorGuard`]), le canal redevient disponible.
//!
//! L'indicateur Discord « ... est en train d'écrire » n'est que le reflet
//! visible de cet état — la source de vérité, fiable, est ici.

use std::collections::HashMap;
use std::sync::Arc;

use fleet_core::ChannelId;
use tokio::sync::{Mutex, OwnedMutexGuard};

/// Gardien du tour de parole : un seul jeton détenu par canal à la fois.
#[derive(Default)]
pub struct FloorControl {
    /// Un verrou par canal, créé à la demande.
    channels: Mutex<HashMap<u64, Arc<Mutex<()>>>>,
}

/// Jeton de parole détenu pour un canal. Libère le canal à la destruction.
///
/// Le champ `_guard` n'est jamais lu : c'est sa **destruction** (drop) qui
/// libère le verrou du canal.
pub struct FloorGuard {
    _guard: OwnedMutexGuard<()>,
    channel: u64,
}

impl FloorGuard {
    /// Identifiant du canal détenu.
    pub fn channel(&self) -> u64 {
        self.channel
    }
}

impl FloorControl {
    /// Crée un floor-control vide.
    pub fn new() -> Self {
        Self::default()
    }

    /// Récupère (ou crée) le verrou associé au canal.
    async fn lock_for(&self, channel: ChannelId) -> Arc<Mutex<()>> {
        let mut map = self.channels.lock().await;
        Arc::clone(
            map.entry(channel.get())
                .or_insert_with(|| Arc::new(Mutex::new(()))),
        )
    }

    /// Acquiert le jeton du canal, en attendant qu'il se libère si nécessaire.
    pub async fn acquire(&self, channel: ChannelId) -> FloorGuard {
        let lock = self.lock_for(channel).await;
        let guard = lock.lock_owned().await;
        FloorGuard {
            _guard: guard,
            channel: channel.get(),
        }
    }

    /// Tente d'acquérir le jeton sans attendre. `None` si le canal est occupé.
    pub async fn try_acquire(&self, channel: ChannelId) -> Option<FloorGuard> {
        let lock = self.lock_for(channel).await;
        lock.try_lock_owned().ok().map(|guard| FloorGuard {
            _guard: guard,
            channel: channel.get(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn ch(n: u64) -> ChannelId {
        ChannelId::new(n).expect("canal non nul")
    }

    #[tokio::test]
    async fn same_channel_is_exclusive() {
        let fc = FloorControl::new();
        let held = fc.acquire(ch(1)).await;
        assert_eq!(held.channel(), 1);
        assert!(
            fc.try_acquire(ch(1)).await.is_none(),
            "canal déjà occupé -> None"
        );
        drop(held);
        assert!(
            fc.try_acquire(ch(1)).await.is_some(),
            "libéré après drop -> Some"
        );
    }

    #[tokio::test]
    async fn different_channels_independent() {
        let fc = FloorControl::new();
        let _g1 = fc.acquire(ch(1)).await;
        assert!(
            fc.try_acquire(ch(2)).await.is_some(),
            "les canaux sont indépendants"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn turn_taking_serializes_same_channel() {
        let fc = Arc::new(FloorControl::new());
        let in_flight = Arc::new(AtomicUsize::new(0));
        let max_in_flight = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();
        for _ in 0..8 {
            let fc = Arc::clone(&fc);
            let in_flight = Arc::clone(&in_flight);
            let max_in_flight = Arc::clone(&max_in_flight);
            handles.push(tokio::spawn(async move {
                let _guard = fc.acquire(ch(7)).await;
                let now = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                max_in_flight.fetch_max(now, Ordering::SeqCst);
                tokio::task::yield_now().await;
                in_flight.fetch_sub(1, Ordering::SeqCst);
            }));
        }
        for h in handles {
            h.await.expect("join");
        }

        assert_eq!(
            max_in_flight.load(Ordering::SeqCst),
            1,
            "un seul détenteur du jeton de parole à la fois"
        );
    }
}
