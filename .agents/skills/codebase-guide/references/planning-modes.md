# Planning & Agent Modes

Eggshell runs several different agents over the same project. They share the
`App` trait (`llm.rs`) but differ in tools, prompt, and orchestration. This
reference covers the planning-oriented ones and how they are wired.

## Mode routing

The desktop sends `mode` with `send_message`; `db.rs::save_exchange` branches on
it. Modes are **plain strings**, not an enum — there is no registry to update.

| `mode` | Entry point | App used |
|---|---|---|
| `iterate` | `iterate::run_iterate_loop` | planner + `AdminPanelApp` + `QaApp` per loop |
| `max` | `max::run_max_breakdown` (phase 1) | `MaxPlannerApp` |
| `plan` | `save_exchange` main path | `PlanningApp` |
| `qa` | `save_exchange` main path | `QaApp` |
| anything else / `implement` | `save_exchange` main path | `AdminPanelApp` |

`iterate` and `max` are intercepted **before** the generic
`(system_prompt, tools)` selection, so their `App` is chosen inside their own
module. Adding a mode means: new `App` in `src-tauri/src/apps/`, an explicit
branch in `save_exchange`, a command if it is long-running, and a frontend
`mode` union member.

## The four `App` implementations

| App | File | Tools |
|---|---|---|
| `AdminPanelApp` (alias `AdminPanel`) | `apps/admin_panel.rs` | full builder set |
| `PlanningApp` | `apps/planning.rs` | `load_skill`, `read_file`, `read_schema` |
| `QaApp` | `apps/qa.rs` | `browser_*`, `lint_code`, read tools |
| `MaxPlannerApp` | `apps/max_planner.rs` | `read_file`, `read_schema` only |

All four return `project_shells()` (`SymfonyShell` + `ReactShell`). Re-exports
live in `apps/mod.rs` and are surfaced through `llm.rs`.

## Plan mode (`PlanningApp`)

- Read-only: writes are impossible, so the output is a document, not code.
- Prompt asks for five required sections: Overview & Requirements, Entities &
  Database Schema, Reports & Dashboards, Navigation & Menu Structure,
  Implementation Steps. The body stays **free-form Markdown**.
- Persistence: `save_exchange` writes `result.content` to
  `<app_data>/plans/project_{projectId}_session_{sessionId}_plan.md` and tags
  the assistant message with `{ isPlan: true, planPath }`.
- UI: `Chat.tsx` renders a plan card (title, saved filename, Markdown body) with
  a **Proceed to Implement** button that re-sends
  `"Proceed with implementation of the approved plan."` in `implement` mode.
- No JSON contract, no task table: the plan is one document for a human to
  approve.

## Max mode (two-phase orchestrator)

For large apps. Phase 1 plans, phase 2 executes one agent per task.

### Phase 1 — breakdown (`max::run_max_breakdown`)

- App: `MaxPlannerApp`, `max_turns: Some(8)`, skills **disabled** via
  `AgentOptions { skills: Some(Vec::new()), .. }`. `None` would append the
  default skills catalogue; the empty vector appends nothing.
- Output contract: **one Markdown document, written once**. There is no fenced
  JSON block any more — an earlier revision asked for prose + JSON and the two
  copies drifted, so the JSON contract was removed.
- Shape: `## Overview`, then one `## N. Title` section per task, each with a
  one-line summary and the sub-sections `### Entity & Fields`,
  `### Screen & Menu`, `### Build Steps`. The planner mirrors the
  `crud_creation` skill flow (`sync_schema` → `ResourcePage` page →
  `ResourceController` → `menu.json`) without having the skill loaded.
- `max::parse_tasks(content)` converts the document to `(title, description)`:
  1. `parse_tasks_from_sections` — canonical. Splits on `## ` (a `### ` line does
     **not** match `strip_prefix("## ")`), strips a leading `N.` / `N)` marker
     with `strip_numbered_prefix`, skips container headings
     (`overview`, `summary`, `tasks`, `task list`, `plan`) via
     `is_container_heading`, drops sections with an empty body, and keeps the
     rest of the body verbatim (sub-headings + bullets) as the description.
  2. `parse_tasks_from_json` — legacy fallback for a fenced `[{title, description}]`.
  3. `parse_tasks_from_lines` — legacy fallback for `1. Title — Description`.
- Persistence: `MaxTasksRepository::replace_for_session` deletes the session's
  rows and inserts the new list into `max_tasks`
  (`0005_create_max_tasks.sql`). Per-task detail lives in the existing
  `description` column — there is no separate details column or migration.
- Emits `max_breakdown_complete` and stores `{ isMaxPlan, maxTaskCount, maxTasks }`
  on the assistant message.

### Phase 2 — execution (`max::run_max_execution`)

- Command `run_max_tasks` builds `MaxControl`, registers the cancel flag in
  `ActiveGenerationMap` / `ActiveMaxMap`, then loops.
- Stops when `MaxTasksRepository::next_unfinished` returns nothing. Unfinished =
  `status IN ('pending','failed','paused')`.
- Each task runs `AdminPanelApp` in a **fresh context** (`max_turns: Some(30)`,
  `cancellation_token: MaxControl.force_stop`) with a prompt built by
  `task_agent_prompt`: overall goal (first user message) + the full ordered task
  list with `<-- YOUR TASK` marker + the task section verbatim, plus an
  instruction to honour the planner's entity/fields and to fall back to the
  `crud_creation` skill if the section is vague.
- Status vocabulary: `pending`, `running`, `completed`, `failed`, `paused`,
  `skipped`.
- Failure handling:
  - `ok` + `hit_max_turns || stalled` → `paused` (resumable), event
    `max_task_paused`, reason `max_turns` / `stalled`.
  - `Err` → `failed`, event `max_task_failed`, then halt the sequence.
  - force-stop → task back to `pending`, phase `stopped`.
- `retry_failed` on `run_max_tasks` calls `MaxTasksRepository::reset_failed`,
  which only resets `failed` (not `paused`).

### Events and commands

| Event | Meaning |
|---|---|
| `max_breakdown_complete` | task list generated (`{ taskCount, tasks }`) |
| `max_execution_start` | phase 2 began |
| `max_task_start` | a task is running |
| `max_task_complete` | task succeeded |
| `max_task_paused` | turn limit or stalled tool call |
| `max_task_failed` | task errored; sequence halts |
| `max_execution_complete` | final task list snapshot |
| `max_status` | `phase`: `stopped` / `halted` / `completed` |

Agent events are wrapped as `{ projectId, sessionId, event, maxStage, maxTaskId }`
so the UI can attribute deltas to a task.

| Command | Purpose |
|---|---|
| `run_max_tasks(projectId, sessionId, retryFailed?)` | run/resume phase 2 |
| `list_max_tasks(projectId, sessionId)` | load persisted tasks |
| `update_max_task_status(taskId, status)` | e.g. mark a stopped task failed from the UI |

## `AgentOptions` knobs that matter for planning modes

- `skills: Option<Vec<Box<dyn Skill>>>` — `None` = default catalogue,
  `Some(Vec::new())` = append no `=== AVAILABLE SKILLS ===` block at all.
- `max_turns: Option<u32>` — planner uses a small budget, implementers larger.
- `cancellation_token: Option<Arc<AtomicBool>>` — shared with mode state maps
  (`Active*Map`) so `stop_chat` / `close` can abort.
- `log_dir` — per-run conversation logs under `<app_log_dir>/agent-conversations`.

## Where prompts live

| Prompt | Location |
|---|---|
| Builder | `apps/admin_panel.rs::system_prompt` |
| Plan mode | `apps/planning.rs::system_prompt` |
| QA | `apps/qa.rs::system_prompt` |
| Max breakdown | `apps/max_planner.rs::system_prompt` + user wrapper in `max.rs` |
| Iterate planner | `iterate.rs::planner_system_prompt` |
| Max per-task | `max.rs::task_agent_prompt` |

**Rule:** a planner prompt must specify exactly **one** output format. Asking for
the same content twice (prose + JSON) doubles output tokens and invites drift;
the machine-readable consumer should parse the single format instead.

## Frontend touchpoints

- `src/MaxTasksCard.tsx` — the Max plan card. Shows the task list with status
  pills, a per-task **Details** disclosure (`splitTaskDescription` separates the
  one-line summary from the `### …` bullets), a **View full plan** disclosure
  rendering the raw Markdown, and Run / Resume / Retry / Stop controls.
- `src/IterationLoopCard.tsx` — iterate-mode progress card.
- `src/Chat.tsx` — decides which card to render per message: `isIterationLoop`,
  then `isMaxPlan`, then `isPlan` (Plan mode), else a plain assistant message.
- `src/App.tsx` — holds `maxTasks` state, listens for the `max_*` events and
  refreshes via `list_max_tasks`; `proceedToImplement` re-sends the plan.
