//! Lecture de secrets compatible Docker Swarm/Compose (pattern `*_FILE`,
//! cf. ADR-008 et ADR-014).
//!
//! Port fidèle du helper éprouvé de Nori-IA (`bot1-noria/crates/core/config.rs`),
//! généralisé au socle de la flotte.

use std::fs;

/// Lit un secret depuis le fichier pointé par `file_env` (pattern Docker
/// `_FILE`), sinon retombe sur la variable d'environnement `value_env`.
///
/// Retourne `Some(valeur trimée)` si l'une des deux sources est utilisable,
/// `None` sinon — le caller décide alors si l'absence est fatale.
///
/// Priorité : le fichier `*_FILE` (s'il est lisible et non vide) prime sur la
/// valeur directe, conformément au pattern secret Docker.
///
/// # Exemple
/// ```
/// use fleet_core::secrets::read_secret_or_env;
/// // En l'absence des deux variables, le résultat est None.
/// let v = read_secret_or_env("ABSENT_FILE_XYZ", "ABSENT_VALUE_XYZ");
/// assert!(v.is_none());
/// ```
pub fn read_secret_or_env(file_env: &str, value_env: &str) -> Option<String> {
    if let Ok(path) = std::env::var(file_env) {
        if let Ok(content) = fs::read_to_string(&path) {
            let trimmed = content.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_owned());
            }
        }
    }
    std::env::var(value_env).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    // Chaque test utilise des noms de variables distincts pour éviter les
    // courses entre tests parallèles (l'environnement de processus est global).

    #[test]
    fn file_wins_over_env() {
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        writeln!(tmp, "  TOKEN_FROM_FILE  ").expect("write");
        std::env::set_var("SEC_T1_FILE", tmp.path());
        std::env::set_var("SEC_T1_VALUE", "FROM_ENV");

        let got = read_secret_or_env("SEC_T1_FILE", "SEC_T1_VALUE");
        assert_eq!(got.as_deref(), Some("TOKEN_FROM_FILE"));
    }

    #[test]
    fn falls_back_to_env() {
        std::env::set_var("SEC_T2_VALUE", "FROM_ENV");
        let got = read_secret_or_env("SEC_T2_FILE_ABSENT", "SEC_T2_VALUE");
        assert_eq!(got.as_deref(), Some("FROM_ENV"));
    }

    #[test]
    fn empty_file_falls_back() {
        let tmp = tempfile::NamedTempFile::new().expect("tempfile");
        std::env::set_var("SEC_T3_FILE", tmp.path());
        std::env::set_var("SEC_T3_VALUE", "FROM_ENV");

        let got = read_secret_or_env("SEC_T3_FILE", "SEC_T3_VALUE");
        assert_eq!(got.as_deref(), Some("FROM_ENV"));
    }

    #[test]
    fn no_value_returns_none() {
        let got = read_secret_or_env("SEC_T4_FILE_ABSENT", "SEC_T4_VALUE_ABSENT");
        assert_eq!(got, None);
    }
}
