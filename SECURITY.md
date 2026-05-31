# Politique de sécurité

## Signaler une vulnérabilité

Merci de **ne pas** ouvrir d'issue publique pour une faille de sécurité.
Contacte le mainteneur en privé (profil GitHub, onglet *Security advisories* du
dépôt, ou message direct) avec une description et, si possible, une reproduction.
Une première réponse est visée sous quelques jours.

## Bonnes pratiques du projet

- **Aucun secret dans le dépôt** : les jetons (Discord, API) sont fournis au
  runtime via des secrets externes ou le pattern de fichiers `*_FILE`, jamais
  committés.
- Le daemon `bot-root` écoute sur un **socket Unix local** — pas d'exposition
  réseau par défaut.
- Les sorties des modèles sont **nettoyées** avant diffusion (`fleet-sanitize` :
  anti-fuite de prompt système, retrait du raisonnement interne).
- Les identifiants sont des **newtypes validés** (rejet du snowflake nul).
