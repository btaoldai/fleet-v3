//! Construction du system prompt final enrichi avec les personas sélectionnés.
//!
//! Port fidèle du `prompt_builder` de Wall-AI, généralisé : la référence à
//! l'opérateur ne cite plus un nom en dur (réutilisable par tous les bots).

use crate::Persona;

/// Construit le system prompt final à injecter dans Ollama.
///
/// Combine `base_prompt` avec les sections des `personas` sélectionnés, et
/// injecte le nom réel et l'ID Discord de l'utilisateur courant.
///
/// Si `personas` est vide, retourne `base_prompt` + contexte utilisateur
/// (fallback gracieux — aucun crash, aucun persona vide injecté).
///
/// # Arguments
/// * `base_prompt` — prompt système de base du bot.
/// * `personas` — personas sélectionnés par le routeur.
/// * `user_name` — nom réel de l'utilisateur qui parle.
/// * `user_id` — ID Discord de l'utilisateur (`0` si inconnu -> section sans ID).
pub fn build_system_prompt(
    base_prompt: &str,
    personas: &[&Persona],
    user_name: &str,
    user_id: u64,
) -> String {
    let user_section = if user_id != 0 {
        format!(
            "---\n\
             ## Contexte utilisateur\n\
             UTILISATEUR ACTUEL : **{user_name}** (ID Discord : {user_id}).\n\
             Tu reponds a cette personne specifiquement. Adapte le niveau de detail \
             et le style a cette personne.\n\
             Si tu dois mentionner l'operateur, utilise son ID configure dans \
             `discord_owner_id` (different de l'utilisateur courant sauf si l'operateur parle)."
        )
    } else {
        format!(
            "---\n\
             ## Contexte utilisateur\n\
             Tu reponds a **{user_name}**. \
             Adapte le niveau de detail et le style a cette personne."
        )
    };

    if personas.is_empty() {
        return format!("{base_prompt}\n\n{user_section}");
    }

    let persona_sections: Vec<&str> = personas.iter().map(|p| p.system_prompt).collect();
    let personas_block = persona_sections.join("\n\n");

    format!(
        "{base_prompt}\n\n\
         {user_section}\n\n\
         ---\n\
         ## Competences activees pour cette requete\n\n\
         {personas_block}"
    )
}

/// Préfixe le prompt système d'une ligne de contexte temporel réel, pour éviter
/// que le modèle invente la date et l'heure (défaut observé : Wall-AI annonçant
/// « 5 juin 2025 » alors qu'on était en 2026).
///
/// `datetime` est fourni déjà formaté par l'appelant (ex. via `chrono`), p. ex.
/// « samedi 31 mai 2026, 09:14 (CEST) ».
pub fn with_datetime_context(system_prompt: &str, datetime: &str) -> String {
    format!(
        "Date et heure actuelles (source fiable, ne jamais inventer) : {datetime}.\n\n\
         {system_prompt}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{REGISTRY, SECU};

    #[test]
    fn datetime_context_prepended() {
        let p = with_datetime_context("Tu es un bot.", "samedi 31 mai 2026, 09:14 (CEST)");
        assert!(p.starts_with("Date et heure actuelles"));
        assert!(p.contains("31 mai 2026"));
        assert!(p.contains("Tu es un bot."));
    }

    fn get(id: &str) -> &'static Persona {
        REGISTRY.iter().copied().find(|p| p.id == id).expect("persona présent")
    }

    #[test]
    fn includes_user_name_and_id() {
        let prompt = build_system_prompt("Tu es un bot.", &[&SECU], "Alice", 987_654_321);
        assert!(prompt.contains("Alice"));
        assert!(prompt.contains("987654321"));
        assert!(prompt.contains("Secu Senior"));
    }

    #[test]
    fn different_users_differ() {
        let a = build_system_prompt("base", &[&SECU], "Alice", 111);
        let b = build_system_prompt("base", &[&SECU], "Bob", 222);
        assert_ne!(a, b);
        assert!(a.contains("UTILISATEUR ACTUEL : **Alice**"));
        assert!(!a.contains("UTILISATEUR ACTUEL : **Bob**"));
    }

    #[test]
    fn no_persona_uses_base_only() {
        let prompt = build_system_prompt("Tu es un bot homelab.", &[], "AnonUser", 0);
        assert!(prompt.contains("Tu es un bot homelab."));
        assert!(prompt.contains("AnonUser"));
        assert!(!prompt.contains("Competences activees"));
    }

    #[test]
    fn zero_user_id_no_id_section() {
        let prompt = build_system_prompt("base", &[], "Anonymous", 0);
        assert!(!prompt.contains("ID Discord : 0"));
    }

    #[test]
    fn multiple_personas_all_included() {
        let prompt = build_system_prompt("base", &[get("secu"), get("rust")], "Bob", 200_000_000_000_000_002);
        assert!(prompt.contains("Secu Senior"));
        assert!(prompt.contains("Rust Expert"));
    }
}
