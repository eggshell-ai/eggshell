# Agent Tools

Tools are the actions the LLM agent can take. They live in
`src-tauri/src/tools/`, are declared and re-exported in `tools/mod.rs`, and are
supplied to an agent through a `struct App`'s `tools()` method (`llm.rs`).

## Catalogue

| File | Type(s) | Tool name | Purpose |
|---|---|---|---|
| `read_file.rs` | `ReadFileTool` | `read_file` | Reads a file within a shell's `src/` directory. Called on a directory it returns that directory's listing plus a warning pointing at `list_directory`. |
| `list_directory.rs` | `ListDirectoryTool` | `list_directory` | Renders a compact indented tree of a directory (dirs first, then files, alphabetical). |
| `ask_user.rs` | `AskUserTool` | `ask_user` | Asks the user a question (optionally with choices) and, in interactive runs, blocks until they answer. |
| `write_file.rs` | `WriteFileTool` | `write_file` | Writes content to a file in a shell. |
| `patch_file.rs` | `PatchFileTool` | `patch_file` | Applies a patch to an existing file. |
| `move_file.rs` | `MoveFileTool` | `move_file` | Moves an uploaded file into a shell. |
| `read_schema.rs` | `ReadSchemaTool` | `read_schema` | Reads written resource schemas from the schema JSON. |
| `sync_schema.rs` | `SyncSchemaTool` | `sync_schema` | Creates/replaces resources (`resources`) or edits them incrementally (`operations`), generates frontend resources + backend entities, and runs migrations. |
| `lint_code.rs` | `LintCodeTool` | `lint_code` | Pluggable checks over a project: PHP syntax, ESLint, `menu.json`, Doctrine mappings, the analytics harness, and Symfony route registration. |
| `load_skill.rs` | `LoadSkillTool` | `load_skill` | Loads a skill, or a specific document of a skill. |
| `manage_user.rs` | `ManageUserTool` | `manage_user` | Creates/drops/modifies/lists project sign-in users. |
| `browser.rs` | `BrowserOpenTool`, `BrowserNavigateTool`, `BrowserClickTool`, `BrowserFillTool`, `BrowserGetContentTool`, `BrowserGetErrorsTool` | `browser_open`, `browser_navigate`, `browser_click`, `browser_fill`, `browser_get_page_content`, `browser_get_console_errors` | Integrated browser automation used by QA. |

## Key conventions

- **Shell-relative paths.** `read_file`/`write_file`/`patch_file`/`move_file`
  take a `shell` of `frontend` or `backend` and a `path` **relative to that
  shell's `src/`** directory. Absolute paths and `..` traversal are rejected.
- **`projectPath` override.** Tools accept an optional `projectPath`; otherwise
  the agent injects the current project directory.
- **Domain-specific tools.** `sync_schema`, `read_schema`, `lint_code`,
  `manage_user`, and the `browser_*` tools encode admin-panel knowledge rather
  than being generic utilities.
- **Registration.** New tools must be declared in `tools/mod.rs` and added to the
  relevant `App::tools()` in `llm.rs`.

See [`resource-system.md`](./resource-system.md) for how `sync_schema` generates
code, and [`rust-backend.md`](./rust-backend.md) for the `App`/`Tool` traits.

## `list_directory`

`list_directory(shell, path?, depth=2, include_hidden=false)` emits **plain text**,
not JSON — an indented tree is cheaper to read and costs fewer tokens. The logic
is split so it can be tested without a project:

- `paths.rs::resolve_shell_path` — shared shell/path validation (extracted from
  `read_file`; also used by `list_directory` and by the `read_file` fallback).
- `list_directory::render_tree(root, depth, include_hidden, max) -> Listing` —
  the pure walker. `tool.execute` only resolves paths and wraps the result.

Design rules worth preserving:

- **Deterministic order.** Directories first, then files, sorted by
  `(lowercase name, raw name)`. Repeated calls are byte-identical, which keeps
  prompt caching effective.
- **Collapsed directories.** `vendor`, `node_modules`, `.git`, `dist`, `build`,
  `.next`, `.nuxt`, `coverage`, plus the Symfony paths `var/cache`, `var/log`,
  `public/build`, `public/bundles`, render as one line:
  `vendor/  [collapsed: 2,481 items]`. They are counted, never walked.
- **Hard cap.** `MAX_ENTRIES = 200`; the remainder collapses into
  `... 1,342 more entries not shown (use a deeper path or a smaller depth)`.
  `max_entries` is intentionally not exposed as a parameter.
- **Symlinks never recurse** (`file_type()` from `read_dir` does not follow them);
  a symlinked directory renders as a file.

## `ask_user`

Blocking request/response between a tool and the UI. `Tool::execute` has no app
handle, so the handshake lives in `src-tauri/src/ask.rs`:

1. `AskBroker::ask` mints a question id, stores a `tokio::sync::oneshot::Sender` in
   a process-wide registry (`OnceLock<Mutex<HashMap<..>>>`, the same pattern as
   `db::pool()` / `Logger::global()`), and emits `agent-event` with
   `event.type = "ask_user"`.
2. The UI renders `src/QuestionCard.tsx`; answering calls the `answer_question`
   Tauri command, which resolves the sender.
3. The parked tool wakes with the answer and returns it as the tool result.

Because it needs the project and session it belongs to, the tool is **not** part
of `App::tools()`: `db.rs::save_exchange` and `max.rs::run_max_breakdown` push an
`AskUserTool` onto the returned `Vec`.

- Interactive modes (implement, plan) pass `interactive = true` and block.
- Max mode passes `interactive = false`: the question is announced but the tool
  returns immediately with `{ answered: false, reason: "unattended…" }`, since
  blocking would hang the whole sequence.
- `stop_chat` calls `ask::cancel_project`, resolving pending questions with
  `AskOutcome::Cancelled` so the run unwinds instead of holding the cancel flag.
  This is the only cancellation path — a tool cannot see `AgentOptions`, and
  there is deliberately no timeout.

## `lint_code` internals

`src-tauri/src/tools/lint_code/` is a small registry of `Check` implementations:

| File | Contents |
|---|---|
| `mod.rs` | `LintCodeTool`: argument validation, check selection, aggregation, JSON output. |
| `check.rs` | `Check` trait, `Shell`, `ShellScope`, `Tier`, `CheckContext`. |
| `issue.rs` | `Severity` + `Issue`; the only place the output JSON keys are named. |
| `registry.rs` | `default_checks()` — the ordered catalogue (static checks first). |
| `php.rs` | `php-syntax`, `doctrine-schema`, `analytics-reports`. |
| `frontend.rs` | `menu-json`, `eslint`. |
| `symfony_routing.rs` | `symfony-route-import` (static) and `symfony-routes` (deep). |

To add a check: implement `Check`, then append it to `default_checks()`. Nothing
else changes — ids appear automatically in the `checks`/`skipChecks` enum and in
the tool description. `Tier::Command` checks that spawn a process should return
`true` from `requires_deep()` so they only run when `deep` is enabled (default
true for `shell: backend|all`, false for `frontend`).

Route checks exist because a controller importing the removed
`Symfony\Component\Routing\Annotation\Route` compiles and passes `php -l`, yet
every `#[Route]` is silently ignored by the attribute loader — `debug:router`
still exits `0` and the only symptom is a 404. `symfony-routes` therefore compares
declared class-level prefixes against the parsed `debug:router --format=json`
table instead of trusting exit codes.
