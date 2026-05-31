# Changelog

Format basé sur [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/),
versionnage selon [SemVer](https://semver.org/lang/fr/).

## [Non publié]

### Ajouté
- Quickstart, exemple `send_request`, workflow CI GitHub Actions, `.env.example`.
- Fichiers de contribution : CONTRIBUTING, SECURITY, gabarit de PR.

## [0.1.0] - 2026-05-31

### Ajouté
- Socle initial d'orchestration LLM mono-GPU en Rust (8 crates).
- `bot-root` : daemon gardien du GPU — slot unique sérialisé (single-flight) +
  cooldown de swap, turn-taking par canal (floor-control), routage par-bot avec
  escalade intra-famille, serveur socket Unix.
- `fleet-bot` : pipeline d'inférence commun (personas + datetime -> bot-root ->
  sanitize -> repli) et guards de comportement (taverne, auto-trigger, cooldowns,
  circuit breaker).
- `fleet-sanitize` : nettoyage des sorties LLM (anti-fuite de prompt, strip
  `<think>`, anti-impersonation, retrait de préfixes de contrôle).
- `fleet-personas` : routage de personnalités par mots-clés + injection de
  contexte temporel.
- `fleet-ollama` : client Ollama unifié. `fleet-core` : erreurs typées, secrets
  `*_FILE`, identifiants validés. `fleet-protocol` : types de fil.
  `bot-root-client` : client IPC léger.
