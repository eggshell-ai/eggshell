# Desktop Frontend (`src/`)

This is the React (Vite) UI of the **Eggshell window itself** — the builder the
user interacts with. It is separate from the generated project frontend
(`templates/admin-panel/`).

Stack: Vite + React 19 + TypeScript. Entry `index.html` → `src/main.tsx`.

## Files

| File | Role |
|---|---|
| `main.tsx` | React root mount. |
| `App.tsx` | Main shell: project/session management, listens for `agent-event` and `project-log`, aggregates streamed messages, handles modes. Invokes `delete_project`, `delete_session`, `start_project`, `wrap_up_chat`, `stop_chat`, `select_model`. |
| `Chat.tsx` | Chat view: messages, tool-run rendering, thinking blocks, model picker, users list. Maps tool names (e.g. `load_skill`, `browser_*`) to friendly labels. Invokes `fetch_models`, `submit_feedback`. |
| `SetupPage.tsx` | First-run setup: dependency detection/install, provider setup, MySQL config. Listens for `dependency-detected`/`progress`. |
| `ProviderSettingsManager.tsx` | Configure LLM providers (`save_provider_config`, `delete_provider`). |
| `SettingsPopup.tsx` | Settings: Telemetry, auto-update, MySQL (`save_auto_update_setting`, `save_mysql_settings`, `save_privacy_settings`). |
| `FeedbackPopup.tsx` | Bug/feature feedback (`submit_feedback`). |
| `ReportMenu.tsx` | Report/export menu; opens URLs via the opener plugin. |
| `UpdatePrompt.tsx` | Updater prompt via `@tauri-apps/plugin-updater` `check` + `plugin-process` `relaunch`. |
| `IterationLoopCard.tsx` | Renders iterate-mode iteration progress cards. |
| `ProjectLogPanel.tsx` | Displays streamed progress lines grouped by channel. |

## Talking to Rust

Two mechanisms:

1. **Commands** — `invoke("<command>", args)` from `@tauri-apps/api`. The command
   names are the `#[tauri::command]` functions in `src-tauri/src/lib.rs`
   (and `config.rs`). Passing invalid args surfaces as a rejected promise.
2. **Events** — `listen("<event>", handler)`. Notable events: `agent-event`
   (streamed agent deltas, thoughts, tool calls/results, token usage) and
   `project-log`/`progress` (multi-channel progress lines from project setup).

### Typical streaming flow

1. `invoke("send_message", ...)` starts an agent run.
2. The backend emits `agent-event` messages as the agent thinks and calls tools.
3. `App.tsx` aggregates them and renders via `Chat.tsx`; `ProjectLogPanel.tsx`
   renders `project-log` lines.

## Adding UI for a new capability

- If you add a Tauri command, expose it here with `invoke` and (if long-running)
  emit progress via the existing channels.
- If you add a tool, add a friendly formatter in `Chat.tsx`'s tool-name map so
  runs read clearly (the `load_skill` formatter is a good example).
