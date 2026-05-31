# Contribuer à fleet-v3

Merci de l'intérêt porté au projet. fleet-v3 est un socle d'orchestration LLM
mono-GPU en Rust pour une flotte de bots partageant un seul GPU.

## Prérequis

- Rust stable ([rustup](https://rustup.rs))
- Pour exécuter le daemon : [Ollama](https://ollama.com) installé

## Avant toute Pull Request

Le code doit passer les mêmes vérifications que la CI :

```bash
cargo build --workspace --examples
cargo clippy --workspace -- -D warnings
cargo test --workspace
cargo fmt --all -- --check
```

## Conventions de code

- **Pas d'`unwrap()` en code de production** (les `.expect()` restent tolérés
  dans les tests).
- **Erreurs typées** via `thiserror` côté bibliothèque.
- **Test-agile** : tout code livré vient avec ses tests dans la même PR ;
  jamais de « tests plus tard ».
- Formatage : `cargo fmt --all` avant de pousser.
- Documentation et commentaires en français ; code et identifiants en anglais.

## Flux de contribution

1. Fork + branche dédiée.
2. Code + tests ; vérifications vertes en local.
3. Pull Request avec une description claire (quoi / pourquoi).
4. La CI doit être verte avant la revue.

## Périmètre et bonnes pistes

Le `Bot` enum et `ModelRegistry::fleet_defaults()` sont **opinionés** sur une
flotte de trois bots. Les contributions qui généralisent ces points (ensemble de
bots configurable, registre chargé depuis un fichier) sont particulièrement
bienvenues.
