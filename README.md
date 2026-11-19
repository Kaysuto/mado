# Mado

> The all-in-one workspace for developers, with the AI model of your choice.

**Mado** (窓, "window" in Japanese) brings mail, calendar, tasks, pull requests, tickets, coding agents and news into one local-first desktop app. You pick the model: a subscription you already pay for, an API key, a free tier, or a model running on your own machine.

Work in progress: the first usable release is not out yet.

[Version française](README.fr.md)

## Development

Requirements: [Rust](https://rustup.rs), Node.js 24, [pnpm](https://pnpm.io) and the [Tauri system dependencies](https://tauri.app/start/prerequisites/) for your platform.

```sh
pnpm install
pnpm tauri dev    # run the app with hot reload
pnpm check        # type-check the interface
pnpm test         # unit tests
```

The interface lives in `src/` (Svelte 5 + TypeScript) and the core in `src-tauri/` (Rust).

## License

[MIT](LICENSE)
