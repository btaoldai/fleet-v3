//! Routeur de personas — matching mots-clés sur la requête utilisateur.
//!
//! Généralisé depuis Wall-AI : le registre et le persona de repli sur requête
//! ambiguë (« prompt-coach ») sont désormais **paramétrés** via [`RouterConfig`]
//! au lieu d'être câblés.

use crate::Persona;

/// Configuration du routeur (généralise les constantes Wall-AI).
#[derive(Debug, Clone)]
pub struct RouterConfig {
    /// Nombre de mots en dessous duquel une requête est considérée « courte ».
    pub short_query_word_threshold: usize,
    /// Verbes/marqueurs d'action signalant une intention claire (désactivent le
    /// repli sur requête ambiguë pour 3..threshold mots).
    pub action_verbs: &'static [&'static str],
    /// Identifiant du persona injecté en tête sur requête courte et ambiguë.
    /// `None` désactive ce comportement.
    pub ambiguity_fallback_id: Option<&'static str>,
}

/// Verbes d'action par défaut (FR + EN) — repris de Wall-AI.
pub const DEFAULT_ACTION_VERBS: &[&str] = &[
    "montre", "explique", "expliques", "crée", "cree", "génère", "genere",
    "fais", "fait", "écris", "ecris", "corrige", "corrigez",
    "analyse", "analyses", "audite", "audites",
    "comment", "pourquoi", "quand", "qu'est", "quelle", "quelles", "quel", "quels",
    "show", "explain", "create", "generate", "write", "fix", "audit",
    "how", "why", "what", "which",
    "aide", "aides", "help", "aide-moi",
    "liste", "lister", "donne", "donnes",
];

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            short_query_word_threshold: 10,
            action_verbs: DEFAULT_ACTION_VERBS,
            ambiguity_fallback_id: Some("prompt-coach"),
        }
    }
}

/// Sélectionne les personas pertinents pour une requête, parmi le `registry`
/// fourni.
///
/// Retourne les `&'static Persona` dont au moins un mot-clé est présent dans
/// `query` (insensible à la casse). Si la requête est courte et ambiguë et que
/// `config.ambiguity_fallback_id` est renseigné, ce persona est injecté en tête.
pub fn select_personas<'a>(
    query: &str,
    registry: &'a [&'a Persona],
    config: &RouterConfig,
) -> Vec<&'a Persona> {
    let lower = query.to_lowercase();
    let mut selected: Vec<&'a Persona> = Vec::new();

    for &persona in registry {
        // Le persona de repli est géré séparément (injection conditionnelle).
        if Some(persona.id) == config.ambiguity_fallback_id {
            continue;
        }
        if persona.keywords.iter().any(|kw| lower.contains(kw)) {
            selected.push(persona);
        }
    }

    if let Some(fallback_id) = config.ambiguity_fallback_id {
        if should_inject_fallback(&lower, config) {
            if let Some(fallback) = registry.iter().copied().find(|p| p.id == fallback_id) {
                selected.insert(0, fallback);
            }
        }
    }

    selected
}

/// Vrai si la requête est assez courte et ambiguë pour déclencher le repli.
///
/// Critères (repris de Wall-AI) : 1-2 mots -> toujours ; 3..threshold mots ->
/// seulement sans verbe d'action ; >= threshold -> jamais.
fn should_inject_fallback(lower_query: &str, config: &RouterConfig) -> bool {
    let word_count = lower_query.split_whitespace().count();
    if word_count >= config.short_query_word_threshold {
        return false;
    }
    if word_count <= 2 {
        return true;
    }
    let has_action_verb = config.action_verbs.iter().any(|v| lower_query.contains(v));
    !has_action_verb
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::REGISTRY;

    #[test]
    fn matches_keyword() {
        let p = select_personas("audit wazuh", REGISTRY, &RouterConfig::default());
        assert!(p.iter().any(|x| x.id == "secu"), "secu attendu : {:?}", ids(&p));
    }

    #[test]
    fn action_verb_prevents_fallback() {
        // "comment ecrire un trait async" : 5 mots, verbe d'action "comment" -> pas de coach.
        let cfg = RouterConfig { ambiguity_fallback_id: Some("coach"), ..Default::default() };
        let p = select_personas("comment ecrire un trait async", REGISTRY, &cfg);
        assert!(p.iter().any(|x| x.id == "rust"), "rust attendu : {:?}", ids(&p));
        assert!(!p.iter().any(|x| x.id == "coach"), "coach non attendu : {:?}", ids(&p));
    }

    #[test]
    fn short_ambiguous_injects_fallback() {
        // "audit" : 1 mot -> fallback injecté ; "audit" matche aussi secu.
        let cfg = RouterConfig { ambiguity_fallback_id: Some("coach"), ..Default::default() };
        let p = select_personas("audit", REGISTRY, &cfg);
        assert_eq!(p.first().map(|x| x.id), Some("coach"), "coach en tête : {:?}", ids(&p));
        assert!(p.iter().any(|x| x.id == "secu"));
    }

    #[test]
    fn no_match_no_domain_persona() {
        let cfg = RouterConfig { ambiguity_fallback_id: Some("coach"), ..Default::default() };
        let p = select_personas("bonjour ca va", REGISTRY, &cfg);
        assert!(!p.iter().any(|x| matches!(x.id, "secu" | "rust")), "aucun persona métier : {:?}", ids(&p));
    }

    #[test]
    fn no_duplicates() {
        let cfg = RouterConfig { ambiguity_fallback_id: Some("coach"), ..Default::default() };
        let p = select_personas("audit wazuh soc pentest", REGISTRY, &cfg);
        let mut sorted = ids(&p);
        sorted.sort_unstable();
        let len_before = sorted.len();
        sorted.dedup();
        assert_eq!(len_before, sorted.len(), "doublons : {:?}", ids(&p));
    }

    fn ids<'a>(p: &[&'a Persona]) -> Vec<&'a str> {
        p.iter().map(|x| x.id).collect()
    }
}
