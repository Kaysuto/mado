# Madow

> Le poste de travail tout-en-un des développeurs, avec le modèle d'IA de leur choix.

**Madow** (de 窓, *mado*, « fenêtre » en japonais) réunit mails, agenda, tâches, demandes de fusion, tickets, agents de code et actus dans une seule application de bureau, qui garde les données sur votre machine. Le modèle est au choix : un abonnement déjà payé, une clé API, une offre gratuite ou un modèle qui tourne sur votre ordinateur.

Aperçu : Madow s'utilise au quotidien mais reste jeune, et des choses bougeront d'une version à l'autre.

## Ce qui marche aujourd'hui

- **Aujourd'hui** : la journée en un coup d'œil, avec un briefing écrit par votre modèle.
- **Boîte** : mails, demandes de relecture, notifications et alertes dans une seule liste, triée par votre modèle rapide. Répondez à un mail à partir d'un brouillon que vous corrigez avant l'envoi.
- **Tâches** : vos tâches et les tickets de vos outils de suivi, ensemble.
- **Agenda** : vos agendas par mois, et de nouveaux événements écrits en langage naturel.
- **Bilan** : le temps de focus par projet, et un bilan écrit du jour ou de la semaine.
- **Assistant** : un chat qui connaît votre journée, avec son historique.
- **Focus** : un chrono lancé depuis n'importe quel élément.

Sources pour l'instant : toute boîte mail (IMAP/SMTP), les agendas par lien iCal, GitHub et Linear. Modèles : Claude Code ou Codex avec votre abonnement, une clé API (Anthropic ou tout service compatible OpenAI), ou un modèle local. Des données d'exemple sont intégrées, pour faire le tour avant de connecter quoi que ce soit.

Vos données restent sur votre machine. Le modèle n'a jamais d'outil dessus : Madow lit vos sources elle-même et lui donne du texte, et rien n'est envoyé ni créé sans votre clic.

## Installation

Téléchargez l'installeur de votre système depuis la [dernière version](https://github.com/Kaysuto/madow/releases).

Les installeurs ne sont pas encore signés, votre système affichera donc un avertissement :

- **Windows** : sur l'avertissement SmartScreen, choisissez « Informations complémentaires », puis « Exécuter quand même ».
- **macOS** : ouvrez l'application une fois, puis autorisez-la dans Réglages Système → Confidentialité et sécurité → « Ouvrir quand même ». Prenez `aarch64` pour Apple Silicon, `x64` pour Intel.
- **Linux** : installez le `.deb` ou le `.rpm`, ou rendez le `.AppImage` exécutable et lancez-le.

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
