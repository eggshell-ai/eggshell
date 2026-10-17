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
| `sync_schema.rs` | `SyncSchemaTool` | `sync_schema` | Generates frontend resources + backend entities from a schema and runs migrations. |
| `lint_code.rs` | `LintCodeTool` | `lint_code` | Lints PHP + JS/TS/TSX + `menu.json`, reporting file/line errors. |
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
