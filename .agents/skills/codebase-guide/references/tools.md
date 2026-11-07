# Agent Tools

Tools are the actions the LLM agent can take. They live in
`src-tauri/src/tools/`, are declared and re-exported in `tools/mod.rs`, and are
supplied to an agent through a `struct App`'s `tools()` method (`llm.rs`).

## Catalogue

| File | Type(s) | Tool name | Purpose |
|---|---|---|---|
| `read_file.rs` | `ReadFileTool` | `read_file` | Reads a file within a shell's `src/` directory. |
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
