# Build, Run & Lint

## Desktop app (repo root)

```powershell
pnpm dev            # Vite dev server only (port 1420)
pnpm build          # tsc && vite build  → ../dist
pnpm preview        # preview the built frontend
pnpm tauri dev      # full desktop dev (runs `pnpm dev` via beforeDevCommand)
pnpm tauri build    # full desktop build (runs `pnpm build` via beforeBuildCommand)
```

> The Tauri dev/build commands invoke the Vite scripts automatically — you
> normally do not run both by hand.

## Rust backend (`src-tauri/`)

```powershell
cargo check         # fast type/compile check
cargo build         # debug build
cargo build --release
cargo test
```

- The library crate is named **`eggshell_lib`** (see `Cargo.toml`).
- SQL migrations under `src-tauri/migrations/` apply automatically via
  `sqlx::migrate!("./migrations")` on startup.
- On Windows PowerShell, `cargo check` may exit non-zero because warnings are
  written to stderr; confirm success by the `Finished ...` line rather than the
  exit code alone.

## Generated project: frontend (`templates/admin-panel/`)

```powershell
npm install
npm run dev         # Next.js dev server on port 4000
npm run build
npm start
npm run lint        # eslint over src/**/*.{js,jsx,ts,tsx}
npm run lint:fix
npm run prettier
```

> The template ships without `node_modules`; install dependencies before linting
> or running it standalone. In normal operation Eggshell copies this template and
> runs `npm install` for you.

## Generated project: backend (`templates/backend/`)

```powershell
composer install
php bin/console <command>          # e.g. doctrine:migrations:migrate
php bin/console lexik:jwt:generate-keypair --overwrite
```

`composer auto-scripts` runs `cache:clear` and `assets:install`. Docker is
available via `compose.yaml`.

## In-agent linting

The agent verifies its own changes with the `lint_code` tool
(`src-tauri/src/tools/lint_code.rs`), which lints PHP, JS/TS/TSX, and
`menu-items/menu.json` and reports file/line errors. Prefer running it after
edits to generated projects rather than relying on manual checks.

See [`tools.md`](./tools.md) for the tool catalogue.
