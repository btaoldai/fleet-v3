# fleet-v3

Orchestrateur d'inférence LLM local pour une flotte de bots Discord partageant
**un seul GPU**, écrit en Rust. Conçu pour un homelab sous contrainte mémoire
forte (un GPU 6 Go de VRAM pour plusieurs bots et plusieurs modèles).

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

## Le problème

Plusieurs bots Discord, chacun avec sa personnalité et son modèle, doivent
partager **un unique GPU de 6 Go**. Un modèle 7-8B en occupe déjà ~5,3 Go : deux
modèles ne tiennent pas ensemble. Sans coordination, le serveur d'inférence
(Ollama) passe son temps à décharger/recharger les modèles à chaque requête
concurrente — le *swap-thrash* — jusqu'au timeout.

## L'approche

Un daemon gardien, **bot-root**, devient le seul composant autorisé à piloter le
GPU. Il sérialise globalement l'accès (un seul modèle résident à la fois),
coordonne les tours de parole entre bots, route chaque requête vers le bon modèle
selon le bot et la complexité, et nettoie les sorties avant envoi.

Principes clés :

- **Slot GPU unique sérialisé** — une seule génération à la fois ; un cooldown de
  *settling* au changement de modèle empêche le swap-thrash.
- **Routage par-bot avec escalade intra-famille** — chaque bot a un modèle par
  défaut rapide (full GPU) et, si la requête est complexe, escalade vers un
  modèle plus gros en *offload* RAM (plus lent, mais exact ; jamais de perte de
  qualité, seulement de la latence). L'escalade reste dans la même famille de
  modèle pour préserver la personnalité.
- **Turn-taking par canal** — un jeton de parole par canal évite que les bots se
  coupent et inondent le fil.
- **Sanitization de sortie** — strip des blocs de raisonnement `<think>`,
  détection d'écho du *system prompt*, anti-impersonation, retrait de préfixes de
  contrôle parasites.

## Architecture (crates)

Workspace Cargo de 8 crates, du plus bas niveau au plus haut :

| Crate | Rôle |
|-------|------|
| `fleet-protocol` | Types de fil partagés (requête/réponse), pur serde |
| `fleet-core` | Erreurs typées, lecture de secrets (pattern `*_FILE`), identifiants validés |
| `fleet-ollama` | Client Ollama unifié (un seul constructeur configurable) |
| `fleet-personas` | Routage de personnalités par mots-clés + injection de contexte temporel |
| `fleet-sanitize` | Nettoyage des sorties LLM (anti-fuite de prompt, anti-`<think>`, etc.) |
| `fleet-bot` | Pipeline d'inférence commun : personas → prompt → bot-root → sanitize → repli |
| `bot-root` | Daemon gardien du GPU : slot-manager, floor-control, routage, serveur socket Unix |
| `bot-root-client` | Client IPC léger dont dépendent les bots (pas du daemon entier) |

Communication bots ↔ daemon : **socket Unix**, JSON ligne-à-ligne.

```
bots ──IPC──> bot-root ──> Ollama (slot GPU unique)
  (bot-root-client)   (slot-manager + floor-control + routage)
```

## Build et tests

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
```

Le binaire daemon :

```bash
cargo run --bin bot-root
```

Configuration par variables d'environnement (valeurs par défaut) :

| Variable | Défaut | Rôle |
|----------|--------|------|
| `BOT_ROOT_SOCKET` | `/tmp/bot-root.sock` | Chemin du socket Unix |
| `OLLAMA_URL` | `http://ollama:11434` | URL d'Ollama |
| `OLLAMA_TIMEOUT_SECS` | `240` | Timeout HTTP (généreux pour l'offload RAM) |
| `BOT_ROOT_SWAP_COOLDOWN_SECS` | `2` | Cooldown de settling au swap de modèle |
| `WINSTON_ESCALATION_MODEL` | (absent) | Modèle d'escalade optionnel selon la VRAM/RAM disponible |

## Statut

Socle complet et testé (~67 tests, clippy strict). Les bots concrets (shell
Discord serenity/poise + capacités spécifiques) se branchent sur `fleet-bot` et
`bot-root-client`.

## Licence

MIT — voir [LICENSE](LICENSE).
