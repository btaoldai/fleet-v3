//! complexity-router — routage par escalade heuristique (décision D3).
//!
//! Stratégie : on part du tier par défaut et on **escalade quand ça chauffe**,
//! sur des signaux peu coûteux (pièce jointe, mots-clés, longueur). Le
//! mini-classifieur (auto-évaluation du modèle) est documenté comme phase 2,
//! activé seulement après validation de cette heuristique simple.
//!
//! Le routeur produit un [`Tier`] ; c'est le [`crate::ModelRegistry`] qui résout
//! ensuite le modèle concret, et qui retombe sur le défaut si le bot ne peut pas
//! escalader (ex. Wall-AI).

use crate::model_registry::Tier;

/// Configuration de l'escalade heuristique.
#[derive(Debug, Clone)]
pub struct EscalationConfig {
    /// Nombre de mots à partir duquel un prompt est jugé « lourd ».
    pub min_words: usize,
    /// Mots-clés déclenchant l'escalade (sous-chaîne, insensible à la casse).
    pub keywords: &'static [&'static str],
}

/// Mots-clés d'escalade par défaut (FR + EN).
pub const DEFAULT_ESCALATION_KEYWORDS: &[&str] = &[
    "audit", "architecture", "analyse", "analyser", "approfondi", "approfondie",
    "expertise", "scientifique", "raisonnement", "demonstration", "démonstration",
    "preuve", "exhaustif", "exhaustive", "audite", "auditer",
];

impl Default for EscalationConfig {
    fn default() -> Self {
        Self {
            min_words: 120,
            keywords: DEFAULT_ESCALATION_KEYWORDS,
        }
    }
}

/// Classe une requête en [`Tier`].
///
/// Escalade si **l'un** de ces signaux est présent : une pièce jointe à analyser,
/// un mot-clé d'escalade, ou un prompt long (≥ `min_words`). Sinon, tier par
/// défaut.
pub fn classify(query: &str, has_attachment: bool, config: &EscalationConfig) -> Tier {
    if has_attachment {
        return Tier::Escalation;
    }
    let lower = query.to_lowercase();
    if config.keywords.iter().any(|kw| lower.contains(kw)) {
        return Tier::Escalation;
    }
    if query.split_whitespace().count() >= config.min_words {
        return Tier::Escalation;
    }
    Tier::Default
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_news_query_stays_default() {
        let t = classify(
            "quels sont les dernieres news tech de la semaine",
            false,
            &EscalationConfig::default(),
        );
        assert_eq!(t, Tier::Default);
    }

    #[test]
    fn audit_keyword_escalates() {
        let t = classify(
            "peux-tu auditer ce document scientifique sur la physique quantique",
            false,
            &EscalationConfig::default(),
        );
        assert_eq!(t, Tier::Escalation);
    }

    #[test]
    fn attachment_escalates() {
        let t = classify("regarde ca", true, &EscalationConfig::default());
        assert_eq!(t, Tier::Escalation);
    }

    #[test]
    fn long_prompt_escalates() {
        let long = "mot ".repeat(130);
        let t = classify(&long, false, &EscalationConfig::default());
        assert_eq!(t, Tier::Escalation);
    }

    #[test]
    fn custom_config_threshold() {
        let cfg = EscalationConfig { min_words: 3, keywords: &[] };
        assert_eq!(classify("un deux", false, &cfg), Tier::Default);
        assert_eq!(classify("un deux trois", false, &cfg), Tier::Escalation);
    }
}
