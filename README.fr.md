# Madow

> Le poste de travail tout-en-un des développeurs, avec le modèle d'IA de leur choix.

**Madow** (de 窓, *mado*, « fenêtre » en japonais) réunit mails, agenda, tâches, demandes de fusion, tickets, agents de code et actus dans une seule application de bureau, qui garde les données sur votre machine. Le modèle est au choix : un abonnement déjà payé, une clé API, une offre gratuite ou un modèle qui tourne sur votre ordinateur.

En cours de construction : la première version utilisable n'est pas encore sortie.

[English version](README.md)

## Développement

Prérequis : [Rust](https://rustup.rs), Node.js 24, [pnpm](https://pnpm.io) et les [dépendances système de Tauri](https://tauri.app/start/prerequisites/) pour votre plateforme.

```sh
pnpm install
pnpm tauri dev    # lance l'application avec rechargement à chaud
pnpm check        # vérifie les types de l'interface
pnpm test         # tests unitaires
```

L'interface est dans `src/` (Svelte 5 + TypeScript) et le cœur dans `src-tauri/` (Rust).

## Licence

[MIT](LICENSE)
