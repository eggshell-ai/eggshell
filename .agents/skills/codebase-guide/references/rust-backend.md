# Rust Backend (`src-tauri/src/`)

The Rust side is the heart of Eggshell: it hosts the Tauri commands the desktop
UI calls, the agent loop, the tools/skills the agent uses, and the shell
implementations that scaffold and run generated projects.

## Module map

| Module | Responsibility & key symbols |
|---|---|
| `lib.rs` | Tauri entry + all `#[tauri::command]` handlers. `run()` builds the app, manages shared state, registers handlers. Helpers `initialize_project(...)`, `start_project(...)`, `start_managed_mysql`. |
| `main.rs` | Initializes Sentry (telemetry-gated) and calls `eggshell_lib::run()`. |
| `llm.rs` | Core traits: `App`, `Shell`, `Skill`, `Tool`, `LLMService`; `AgentOptions`, `StreamChunk`; app definitions `AdminPanelApp`, `PlanningApp`, `QaApp`; `default_skills()`. |
| `agent.rs` | `AgentService` — the agent loop (`run_agent`), streaming, tool dispatch, conversation logging, `attach_skills_summary`, `attach_artifacts`. |
| `skill_loader.rs` | `MarkdownSkill` + `load_all()` — embeds `src/skills/**` with `include_dir!`, parses `SKILL.md` frontmatter, and serves bundled files by path. |
| `skills/<skill>/SKILL.md` | One folder per runtime skill, with optional supporting files (e.g. `docs/*.md`) alongside it (see [`skill-system.md`](./skill-system.md)). |
| `tools/` | Agent tools (see [`tools.md`](./tools.md)). |
| `react.rs` / `symfony.rs` | `Shell` implementations that copy templates, install deps, and start dev servers. |
| `db.rs` + `migrations/` | SQLite via `sqlx`; repositories `ProjectsRepository`, `SessionsRepository`, `UsersRepository`; SQL migrations in `src-tauri/migrations/`. |
| `setup.rs`, `dependencies.rs` | Managed toolchain paths (PHP/Node/MySQL under `%APPDATA%/eggshell`), dependency detection/install. |
| `config.rs` | `AppConfig` (version, providers, mysql, flags), `ConfigService`, `current_version()`. |
| `telemetry.rs` | Enable/disable telemetry, scope config, record events. |
| `logger.rs` | Central in-memory `Logger` with a broadcast channel and `global()` mirror. |
| `progress.rs` | `ProgressLog` with multi-channel streaming (`LogLine`), emits `project-log`/`progress` events. |
| `iterate.rs` | The Iterate auto-improvement loop (`run_iterate_loop`, `IterationControl`, planner/implementer/QA phases). |
| `providers/` | LLM providers: `mod.rs` (`ProviderHub`, `registered_providers`, `build_service`), `openai.rs` (`OpenAiService`), `ollama.rs` (`OllamaService`). |
| `mock_llm.rs` | `MockLlmService` for tests. |

## Core traits (`llm.rs`)

- `trait App` — `shells()`, `tools()`, `skills()`, `system_prompt()`.
- `trait Shell` — `init(project_path, slug, template_root, log, mysql_password, is_mariadb)` and `start(project_path)`.
- `trait Skill` — `name`, `description`, `content`, `category`, `tags`, `read(path)`, `files()`.
- `struct MarkdownSkill` — the single `Skill` implementation, built from a skill folder's `SKILL.md`.
- `trait Tool` — `name`, `description`, `parameters`, `execute`.
- `trait LLMService` — `execute_prompt`, `execute_prompt_with_tools`, `execute_prompt_with_tools_cancellable`, `list_models`.

## App definitions

Three `App` implementations define what an agent can do:

- **`AdminPanelApp`** (alias `AdminPanel`) — the builder. Shells
  `[SymfonyShell, ReactShell]`; tools include the full file/schema/lint/user set;
  a large `system_prompt()` describing Next.js App Router pages and
  `menu-items/menu.json`.
- **`PlanningApp`** — read-only planner. Tools: `load_skill`, `read_file`,
  `read_schema`. Produces structured Markdown plans, no writes.
- **`QaApp`** — visual QA. Tools include the `browser_*` tools + `lint_code`.

## Tauri commands (`lib.rs`)

Grouped roughly:

- **Setup/deps**: `greet`, `detect_dependencies`, `install_dependency`, `setup_log_history`, `read_logs`
- **Config/providers**: `load_setup_state`, `save_privacy_settings`, `save_auto_update_setting`, `config::save_mysql_config`, `config::save_mysql_settings`, `save_provider_config`, `test_provider_config`, `delete_provider`, `select_model`, `fetch_models`
- **Projects**: `list_projects`, `create_project`, `delete_project`, `start_project`
- **Users/sessions/chat**: `list_users`, `list_sessions`, `delete_session`, `send_message`, `wrap_up_chat`, `stop_chat`, `compact_session`
- **Misc**: `submit_feedback`, `get_app_version`

The frontend calls these with Tauri `invoke("<command>", args)`.

## Shells

- **`react.rs` — `ReactShell`**: copies `templates/admin-panel` into the project's
  `frontend/`, runs `npm install`, starts `npm run dev`. Logs on the `react`
  channel. Helpers: `copy_template`, `run_npm_install`, `start_dev_server`,
  `npm_command`.
- **`symfony.rs` — `SymfonyShell`**: copies `templates/backend` into `backend/`
  (excluding `composer.lock`), runs `composer install`, generates JWT keypairs
  (`lexik:jwt:generate-keypair`), runs additional `bin/console` commands, and
  starts the server. Helpers: `copy_template`, `run_php_command`, `new_php_command`
  (uses managed PHP from `setup.rs`), `run_php_capture`/`run_php_capture_blocking`.

## Agent loop (`agent.rs`)

`AgentService::run_agent(messages, tools, options)`:

1. Attaches the skills summary and artifact names to the messages.
2. Iterates up to `DEFAULT_MAX_TURNS`, streaming deltas via a callback.
3. Executes tool calls (injecting `project_path`) and appends results.
4. Emits events (`thought_delta`, `content_delta`, `thought`, `tool_call`,
   `tool_result`, `token_usage`, `complete`), honors cancellation, and writes a
   conversation log.

See [`skill-system.md`](./skill-system.md) for how skills reach the model, and
[`tools.md`](./tools.md) for the tool catalogue.
