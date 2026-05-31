//! Identifiants Discord validés (newtypes non-nuls, type-safe).
//!
//! Un snowflake Discord est un entier strictement positif. Ces newtypes
//! garantissent à la construction qu'on ne manipule jamais l'identifiant nul,
//! et empêchent de confondre un identifiant d'utilisateur, de canal ou de bot
//! (doctrine baptiste-code-style : newtypes validés).

use std::num::NonZeroU64;

use crate::error::{CoreError, Result};

macro_rules! discord_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(NonZeroU64);

        impl $name {
            /// Construit l'identifiant. Échoue si `raw == 0`
            /// ([`CoreError::InvalidDiscordId`]).
            pub fn new(raw: u64) -> Result<Self> {
                NonZeroU64::new(raw)
                    .map($name)
                    .ok_or(CoreError::InvalidDiscordId)
            }

            /// Valeur brute du snowflake.
            pub fn get(self) -> u64 {
                self.0.get()
            }
        }

        impl TryFrom<u64> for $name {
            type Error = CoreError;
            fn try_from(raw: u64) -> Result<Self> {
                Self::new(raw)
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0.get())
            }
        }
    };
}

discord_id!(
    /// Identifiant d'un utilisateur Discord.
    UserId
);
discord_id!(
    /// Identifiant d'un canal Discord.
    ChannelId
);
discord_id!(
    /// Identifiant d'un bot de la flotte.
    BotId
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_zero() {
        assert!(UserId::new(0).is_err());
        assert!(ChannelId::try_from(0).is_err());
        assert!(BotId::new(0).is_err());
    }

    #[test]
    fn accepts_positive_and_roundtrips() {
        let id = BotId::new(100_000_000_000_000_001).expect("non nul");
        assert_eq!(id.get(), 100_000_000_000_000_001);
        assert_eq!(id.to_string(), "100000000000000001");
    }

    #[test]
    fn try_from_ok() {
        let c = ChannelId::try_from(42_u64).expect("non nul");
        assert_eq!(c.get(), 42);
    }
}
