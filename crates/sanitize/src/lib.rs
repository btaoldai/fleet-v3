//! fleet-sanitize — nettoyage des sorties LLM avant envoi sur Discord.
//!
//! Corrige des défauts observés en production (audit pré-MEP 2026-05-31) :
//! - fuite des blocs de raisonnement `<think>...</think>` (qwen3 thinking) ;
//! - fuite/écho du system prompt (persona recraché verbatim) ;
//! - bot qui parle au nom d'un autre bot (impersonation) ;
//! - préfixes de contrôle parasites (`[MSG:]`, `[Px]` hors mode incident).
//!
//! Toutes les fonctions sont **pures** (pas d'I/O, pas de dépendance externe),
//! donc faciles à tester sur les vrais extraits de l'échange audité.
//!
//! # Exemple
//! ```
//! use fleet_sanitize::{sanitize, SanitizeConfig};
//!
//! let cfg = SanitizeConfig::new("Winston").other_bots(["Wall-AI", "NoriA"]);
//! let out = sanitize("<think>je réfléchis</think>Bonjour !", &cfg);
//! assert_eq!(out.text, "Bonjour !");
//! assert!(out.think_stripped);
//! ```

/// Retire tous les blocs `<think>...</think>` du texte.
///
/// Gère plusieurs blocs et un bloc ouvert sans fermeture (tout ce qui suit un
/// `<think>` non fermé est jeté).
pub fn strip_think_blocks(text: &str) -> String {
    const OPEN: &str = "<think>";
    const CLOSE: &str = "</think>";

    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        match rest.find(OPEN) {
            Some(start) => {
                out.push_str(&rest[..start]);
                let after = &rest[start + OPEN.len()..];
                match after.find(CLOSE) {
                    Some(end) => rest = &after[end + CLOSE.len()..],
                    None => break, // bloc non fermé : on jette le reste
                }
            }
            None => {
                out.push_str(rest);
                break;
            }
        }
    }
    out.trim().to_string()
}

/// Retire les préfixes de contrôle en tête de message.
///
/// `[MSG:]` est toujours retiré. Les tags de priorité `[P0]`..`[P9]` ne sont
/// retirés que si `allow_incident_tags` est faux (en mode incident, ils sont
/// légitimes).
pub fn strip_control_prefixes(text: &str, allow_incident_tags: bool) -> String {
    let mut s = text.trim_start();
    loop {
        let before = s;
        if let Some(stripped) = s.strip_prefix("[MSG:]") {
            s = stripped.trim_start();
        }
        if !allow_incident_tags {
            if let Some(rest) = strip_priority_tag(s) {
                s = rest.trim_start();
            }
        }
        if s == before {
            break;
        }
    }
    s.to_string()
}

/// Retire un éventuel tag `[P<chiffre>]` en tête. Retourne le reste, ou `None`.
fn strip_priority_tag(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    if bytes.len() >= 4 && bytes[0] == b'[' && bytes[1] == b'P' && bytes[2].is_ascii_digit() && bytes[3] == b']' {
        Some(&s[4..])
    } else {
        None
    }
}

/// Coupe le texte dès qu'une ligne est attribuée à un autre bot (impersonation).
///
/// Détecte les lignes commençant par `AutreBot :` ou `AutreBot:` (insensible à
/// la casse) et tronque à partir de là. Retourne `(texte, true)` si une coupe a
/// eu lieu.
pub fn strip_foreign_speaker_lines(text: &str, others: &[&str]) -> (String, bool) {
    let mut kept: Vec<&str> = Vec::new();
    let mut stripped = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if others.iter().any(|name| starts_with_speaker_label(trimmed, name)) {
            stripped = true;
            break;
        }
        kept.push(line);
    }
    (kept.join("\n").trim().to_string(), stripped)
}

/// Vrai si la ligne commence par `name` suivi de `:` (insensible à la casse).
fn starts_with_speaker_label(line: &str, name: &str) -> bool {
    let line_l = line.to_lowercase();
    let name_l = name.to_lowercase();
    match line_l.strip_prefix(name_l.as_str()) {
        Some(rest) => rest.trim_start().starts_with(':'),
        None => false,
    }
}

/// Vrai si le texte contient au moins `threshold` marqueurs-signature du persona
/// (heuristique de détection d'écho du system prompt).
pub fn detect_persona_leak(text: &str, markers: &[&str], threshold: usize) -> bool {
    if threshold == 0 || markers.is_empty() {
        return false;
    }
    let lower = text.to_lowercase();
    let hits = markers
        .iter()
        .filter(|m| lower.contains(m.to_lowercase().as_str()))
        .count();
    hits >= threshold
}

/// Configuration de [`sanitize`].
#[derive(Debug, Clone)]
pub struct SanitizeConfig {
    /// Retirer les blocs `<think>`.
    pub strip_think: bool,
    /// Conserver les tags `[Px]` (mode incident).
    pub allow_incident_tags: bool,
    /// Nom du bot courant (exclu de la détection d'impersonation).
    pub own_name: String,
    /// Noms des autres bots de la flotte (détection d'impersonation).
    pub other_bot_names: Vec<String>,
    /// Phrases-signature du persona du bot (détection d'écho du prompt).
    pub persona_markers: Vec<String>,
    /// Nombre de marqueurs à partir duquel on considère qu'il y a fuite.
    pub persona_leak_threshold: usize,
}

impl SanitizeConfig {
    /// Configuration par défaut pour un bot donné (strip_think actif,
    /// pas de mode incident, seuil de fuite à 3).
    pub fn new(own_name: impl Into<String>) -> Self {
        Self {
            strip_think: true,
            allow_incident_tags: false,
            own_name: own_name.into(),
            other_bot_names: Vec::new(),
            persona_markers: Vec::new(),
            persona_leak_threshold: 3,
        }
    }

    /// Active la conservation des tags `[Px]` (contexte incident).
    pub fn allow_incident_tags(mut self, allow: bool) -> Self {
        self.allow_incident_tags = allow;
        self
    }

    /// Déclare les autres bots de la flotte.
    pub fn other_bots(mut self, names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.other_bot_names = names.into_iter().map(Into::into).collect();
        self
    }

    /// Déclare les phrases-signature du persona.
    pub fn persona_markers(mut self, markers: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.persona_markers = markers.into_iter().map(Into::into).collect();
        self
    }

    /// Fixe le seuil de détection de fuite persona.
    pub fn persona_leak_threshold(mut self, threshold: usize) -> Self {
        self.persona_leak_threshold = threshold;
        self
    }
}

/// Résultat de [`sanitize`] : texte nettoyé + drapeaux de diagnostic.
#[derive(Debug, Clone)]
pub struct SanitizeOutcome {
    /// Texte nettoyé, prêt à envoyer (sauf si [`SanitizeOutcome::is_unsafe`]).
    pub text: String,
    /// Un bloc `<think>` a été retiré.
    pub think_stripped: bool,
    /// Un préfixe de contrôle a été retiré.
    pub prefixes_stripped: bool,
    /// Une portion attribuée à un autre bot a été retirée.
    pub impersonation_stripped: bool,
    /// Une fuite du system prompt a été détectée.
    pub persona_leak_detected: bool,
}

impl SanitizeOutcome {
    /// Vrai si un problème grave (fuite persona) a été détecté : l'appelant
    /// devrait **régénérer** ou envoyer un repli plutôt que d'afficher `text`.
    pub fn is_unsafe(&self) -> bool {
        self.persona_leak_detected
    }
}

/// Nettoie une réponse LLM selon la configuration.
pub fn sanitize(raw: &str, config: &SanitizeConfig) -> SanitizeOutcome {
    let think_present = raw.contains("<think>");
    let mut text = if config.strip_think {
        strip_think_blocks(raw)
    } else {
        raw.trim().to_string()
    };

    let before_prefix = text.clone();
    text = strip_control_prefixes(&text, config.allow_incident_tags);
    let prefixes_stripped = text != before_prefix;

    let others: Vec<&str> = config.other_bot_names.iter().map(String::as_str).collect();
    let (cleaned, impersonation_stripped) =
        strip_foreign_speaker_lines(&text, &others);
    text = cleaned;

    let markers: Vec<&str> = config.persona_markers.iter().map(String::as_str).collect();
    let persona_leak_detected = detect_persona_leak(&text, &markers, config.persona_leak_threshold);

    SanitizeOutcome {
        text,
        think_stripped: think_present && config.strip_think,
        prefixes_stripped,
        impersonation_stripped,
        persona_leak_detected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_single_think_block() {
        let input = "<think>raisonnement\nmulti-lignes</think>Voici la réponse.";
        assert_eq!(strip_think_blocks(input), "Voici la réponse.");
    }

    #[test]
    fn strips_dangling_think() {
        // Bloc ouvert sans fermeture : tout ce qui suit est jeté.
        let input = "Bonjour <think>je réfléchis sans jamais fermer la balise";
        assert_eq!(strip_think_blocks(input), "Bonjour");
    }

    #[test]
    fn strips_multiple_think_blocks() {
        let input = "<think>a</think>X<think>b</think>Y";
        assert_eq!(strip_think_blocks(input), "XY");
    }

    #[test]
    fn strips_msg_and_priority_prefix() {
        assert_eq!(strip_control_prefixes("[MSG:]Foi de DevOps", false), "Foi de DevOps");
        assert_eq!(strip_control_prefixes("[P2] Erreur Ollama", false), "Erreur Ollama");
    }

    #[test]
    fn keeps_priority_tag_in_incident_mode() {
        // En mode incident, [P2] reste ; [MSG:] est toujours retiré.
        assert_eq!(strip_control_prefixes("[P2] Erreur Ollama", true), "[P2] Erreur Ollama");
        assert_eq!(strip_control_prefixes("[MSG:][P2] Erreur", true), "[P2] Erreur");
    }

    #[test]
    fn cuts_foreign_speaker_lines() {
        // Cas réel : Winston répond pour les trois bots.
        let input = "Winston : \"Je suis Winston, le DevOps senior.\"\n\
                     Wall-AI : \"Je suis Wall-AI, le N1.\"\n\
                     NoriA : \"Je suis NoriA, le nain forgeron.\"";
        let (text, stripped) = strip_foreign_speaker_lines(input, &["Wall-AI", "NoriA"]);
        assert!(stripped);
        assert!(text.contains("Je suis Winston"));
        assert!(!text.contains("Wall-AI"));
        assert!(!text.contains("nain forgeron"));
    }

    #[test]
    fn no_false_positive_on_mention() {
        // Mentionner un autre bot en milieu de phrase n'est pas de l'impersonation.
        let input = "Pour ça, il te faut Winston ! À toi de jouer.";
        let (text, stripped) = strip_foreign_speaker_lines(input, &["Winston", "Wall-AI"]);
        assert!(!stripped);
        assert_eq!(text, input);
    }

    #[test]
    fn detects_persona_prompt_echo() {
        // Extrait réel de la fuite Nori-IA.
        let leak = "Un nain ne révèle pas les secrets de sa forge. Je suis Nori, le frère \
                    cadet de Dori. Rust est mon acier qui ne rouille pas.";
        let markers = ["un nain ne révèle pas", "frère cadet de Dori", "acier qui ne rouille pas"];
        assert!(detect_persona_leak(leak, &markers, 2));

        let normal = "Par la barbe de Durin ! J'ai forgé un script pour toi.";
        assert!(!detect_persona_leak(normal, &markers, 2));
    }

    #[test]
    fn sanitize_full_pipeline() {
        let cfg = SanitizeConfig::new("Winston")
            .other_bots(["Wall-AI", "NoriA"])
            .persona_markers(["frère cadet de Dori"])
            .persona_leak_threshold(1);

        let raw = "<think>analyse interne</think>[MSG:]Voici la réponse propre.";
        let out = sanitize(raw, &cfg);
        assert_eq!(out.text, "Voici la réponse propre.");
        assert!(out.think_stripped);
        assert!(out.prefixes_stripped);
        assert!(!out.impersonation_stripped);
        assert!(!out.is_unsafe());
    }

    #[test]
    fn sanitize_flags_persona_leak() {
        let cfg = SanitizeConfig::new("NoriA")
            .persona_markers(["un nain ne révèle pas", "frère cadet de Dori"])
            .persona_leak_threshold(2);
        let out = sanitize(
            "Un nain ne révèle pas les secrets. Je suis le frère cadet de Dori.",
            &cfg,
        );
        assert!(out.is_unsafe(), "la fuite persona doit être signalée");
    }
}
