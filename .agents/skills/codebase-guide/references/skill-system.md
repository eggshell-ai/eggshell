# Eggshell Runtime Skill System

This documents **Eggshell's own runtime skills** — the instructions the *LLM
agent* loads while building user apps. This is distinct from VS Code agent
skills (`SKILL.md` folders under `.agents/skills/`).

## Concept

A **skill** is a named bundle of markdown instructions. The agent learns what
skills exist up front (a summary is injected into the system prompt), then pulls
a skill's full content on demand by calling the `load_skill` tool. A skill may
also expose **documents** — smaller reference files fetched individually so the
main instructions stay concise.

## End-to-end flow

1. **Definition** — A skill is a Rust type implementing `trait Skill`
   (`src-tauri/src/llm.rs`):
   ```rust
   pub trait Skill: Send + Sync {
       fn name(&self) -> &str;
       fn description(&self) -> &str;
       fn content(&self) -> &str;
       fn category(&self) -> Option<&str>;
       fn tags(&self) -> &[String];
       fn documents(&self) -> &[SkillDocument] { &[] } // default empty
   }
   ```
2. **Content source** — Each skill module stores its markdown filename and
   returns the text via `skill_markdown::load(...)`, which uses
   `include_str!("skills/<file>.md")` to embed it **at compile time**
   (`src-tauri/src/skill_markdown.rs`).
3. **Bundling** — `default_skills()` in `llm.rs` returns the concrete skills;
   the `App` implementations pass them to `LoadSkillTool::new(default_skills())`.
4. **Registration in the prompt** — `AgentService::attach_skills_summary`
   (`src-tauri/src/agent.rs`) appends an `=== AVAILABLE SKILLS ===` block to the
   system message listing each skill's name/description/category/tags **and its
   documents** (name + description), then tells the model to call `load_skill`.
5. **On-demand fetch** — `LoadSkillTool` (`src-tauri/src/tools/load_skill.rs`,
   tool name `load_skill`) accepts `{ skillName, document? }`:
   - without `document`, returns the skill's metadata (and lists its documents);
   - with `document`, returns that `SkillDocument`'s `content`;
   - unknown skill/document returns a structured error with the available options.

## The `documents()` / `SkillDocument` mechanism

```rust
pub struct SkillDocument {
    pub name: &'static str,        // e.g. "row_actions"
    pub description: &'static str, // shown in the skills summary
    pub content: &'static str,     // embed via skill_markdown::load("...")
}
```

Purpose: let a skill keep its primary `content` short while shipping deeper
reference material that the agent loads only when needed (progressive
disclosure). Example: `CrudCreationSkill` bundles the documents `row_actions`
and `custom_ui`.

**Gotcha:** `include_str!` results cannot be used in a `const` (calling
`skill_markdown::load` in a `const` fails to compile). Store documents in the
skill struct, built in `new()` (as `CrudCreationSkill` does), not in a `const`.

## Concrete skills

| Skill name | Rust module | Markdown | Documents |
|---|---|---|---|
| `crud_creation` | `crud_creation.rs` | `skills/crud_creation.md` | `row_actions`, `custom_ui` |
| `analytics_and_reporting` | `analytics_and_reporting.rs` | `skills/analytics_and_reporting.md` | — |
| `customization_branding` | `customization_branding.rs` | inline `const CONTENT` | — |
| `notification_creation` | `notification_creation.rs` | `skills/notification_creation.md` | — |
| `profile_management` | `profile_management.rs` | `skills/profile_management.md` | — |
| `roles_and_permissions` | `roles_and_permissions.rs` | `skills/roles_and_permissions.md` | — |
| `qa_testing` | `qa_testing.rs` | `skills/qa_testing.md` | — |

> Note `customization_branding.rs` differs: it stores its markdown in an inline
> `const CONTENT: &str` rather than a `skills/*.md` file.

## How to add or change a runtime skill

1. Add or edit the markdown under `src-tauri/src/skills/`.
2. Register the filename in `skill_markdown::load` (the `match` arms).
3. Add a `struct XxxSkill` implementing `Skill` in a matching module (mirror an
   existing one), wiring `content()` (and `documents()` if needed).
4. Declare the module and re-export it in `llm.rs` (via `#[path = "..."]`).
5. Add it to `default_skills()` in `llm.rs`.

## How to add a skill document

1. Create the markdown under `src-tauri/src/skills/` (e.g. `my_topic.md`).
2. Register it in `skill_markdown::load`.
3. Add a `SkillDocument { name, description, content: skill_markdown::load(...) }`
   to the owning skill's `documents()`.
4. Mention/point to it from the skill's main markdown so the agent knows when to
   load it (`load_skill({ skillName, document })`).

## UI touchpoint

The desktop chat UI renders a friendly label for the tool call; the `load_skill`
formatter lives in `src/Chat.tsx` and shows the document name when present.
