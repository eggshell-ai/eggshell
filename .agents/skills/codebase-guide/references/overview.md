# Overview & Tech Stack

Eggshell is a **Tauri 2 desktop application**: a Rust backend with a Vite/React
frontend. Its purpose is to act as a *platform-guided AI app builder* — it
scaffolds and iteratively builds "admin-panel" web applications and drives LLM
agents (with tools and skills) to extend them.

## Top-level layout

| Path | Responsibility |
|---|---|
| `src/` | React (Vite) frontend for the **Eggshell window itself** (the builder UI). |
| `src-tauri/` | **Rust backend** (Tauri): commands, agent loop, tools, skills, shell implementations. |
| `templates/` | Scaffolding copied into each **generated project**: `admin-panel/` (Next.js) and `backend/` (Symfony). |
| `public/` | Static assets for the Vite dev server. |
| `.agents/skills/` | VS Code agent skills (this guide + `bump-version`). |

## Root manifests

- **`package.json`** — name `eggshell`, `type: module`. Scripts: `dev` (`vite`),
  `build` (`tsc && vite build`), `preview`, `tauri`. Deps include
  `@tauri-apps/api`, plugins `dialog`/`opener`/`process`/`updater`,
  `@latentic/live-markdown`, `react` 19, `react-markdown`, `remark-gfm`.
- **`vite.config.ts`** — Tauri-aware Vite config: React plugin, fixed port
  **1420** (`strictPort`), HMR port 1421, ignores `**/src-tauri/**`,
  `clearScreen: false`.
- **`tsconfig.json`** — strict TS, `jsx: react-jsx`, bundler resolution, `noEmit`,
  includes `src`.
- **`src-tauri/tauri.conf.json`** — `productName: eggshell`, identifier
  `gd.rf.faheemanis.eggshell`. Dev uses `http://localhost:1420`; build outputs to
  `../dist`. Bundles `../templates/**/*` as resources. Updater points at the
  `eggshell-ai/eggshell` GitHub releases.
- **`src-tauri/Cargo.toml`** — lib/crate name **`eggshell_lib`** (must differ from
  the bin name; enforced by `[lib] name`). Edition 2021. Notable deps: `tauri` 2,
  `tauri-plugin-{opener,dialog,updater,process}`, `serde`/`serde_json`/`serde_yaml`,
  `semver`, `thiserror`, `async-trait`, `reqwest`, `sqlx` (sqlite), `tokio`,
  `tokio-stream`, `eventsource-stream`, `sentry`, `uuid`, `tokio-tungstenite`,
  `lazy_static`, `futures-util`.
- **`src-tauri/src/main.rs`** — initializes Sentry (gated by telemetry/feedback
  settings) then calls `eggshell_lib::run()`.

## The two halves

1. **The desktop shell** (this repo) — the Rust + React app the user interacts
   with. It manages projects/sessions, runs agents, and streams output.
2. **Generated projects** — the admin-panel apps created from `templates/`. They
   live outside this repo (in the user's chosen project directory) and are edited
   by the agent's tools.

## Where to go next

- Rust internals → [`rust-backend.md`](./rust-backend.md)
- Skill system → [`skill-system.md`](./skill-system.md)
- Tools → [`tools.md`](./tools.md)
- Desktop UI → [`desktop-frontend.md`](./desktop-frontend.md)
- Templates → [`templates.md`](./templates.md)
- Resource/CRUD → [`resource-system.md`](./resource-system.md)
- Commands → [`build-and-run.md`](./build-and-run.md)
