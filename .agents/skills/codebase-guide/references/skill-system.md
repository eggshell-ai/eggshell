# Eggshell Runtime Skill System

This documents **Eggshell's own runtime skills** — the instructions the *LLM
agent* loads while building user apps. This is distinct from VS Code agent
skills (`SKILL.md` folders under `.agents/skills/`), though both use the same
folder standard.

## Concept

A **skill** is a folder under `src-tauri/src/skills/` that contains a
`SKILL.md` file. The agent learns what skills exist up front (a summary is
injected into the system prompt), then pulls a skill's instructions on demand by
calling the `load_skill` tool.

A skill may also keep supporting files next to its `SKILL.md` (conventionally
under `docs/`). Those files are **not advertised** in the prompt: `SKILL.md`
itself links to them, and the agent reads one by passing its relative path to
`load_skill`.

```
src-tauri/src/skills/
  crud_creation/
    SKILL.md
    docs/
      row_actions.md
      custom_ui.md
      hooks_and_lifecycle.md
  analytics_and_reporting/
    SKILL.md
  ...
```

## SKILL.md format

Metadata comes from YAML frontmatter. Only `name` and `description` are
practically required; `category` and `tags` are optional and omitted from the
prompt when absent.

```markdown
---
name: crud_creation
description: Teaches the agent how to create a complete CRUD interface.
category: development
tags:
  - crud
  - database
---

# CRUD Creation Skill

...
```

The folder name is the fallback for `name`, and the first `# Heading` is the
fallback for `description`. Frontmatter is stripped before the body is returned
to the agent.

## End-to-end flow

1. **Embedding** — `skill_loader.rs` embeds the whole `src/skills` tree at
   compile time with `include_dir!`. No per-file registration is needed, so
   adding a document requires no Rust change.
2. **Discovery** — `load_all()` takes every direct subdirectory containing a
   `SKILL.md`, parses its frontmatter with `serde_yaml`, and returns the skills
   sorted by name.
3. **Bundling** — `default_skills()` in `llm.rs` delegates to
   `skill_loader::load_all()`; each `App` passes that list to
   `LoadSkillTool::new(default_skills())`.
4. **Registration in the prompt** — `AgentService::attach_skills_summary`
   (`src-tauri/src/agent.rs`) appends an `=== AVAILABLE SKILLS ===` block listing
   each skill's name/description/category/tags. Documents are **not** listed.
5. **On-demand fetch** — `LoadSkillTool` (`src-tauri/src/tools/load_skill.rs`)
   accepts `{ skillName, path? }`:
   - without `path`, returns the skill metadata and `SKILL.md` body;
   - with `path` (e.g. `docs/row_actions.md`), returns that bundled file;
   - unknown skills/files return a structured error, and unknown paths include
     the list of bundled `files()`.

Paths are resolved inside the skill folder only. Absolute paths, drive letters,
and `.`/`..` segments are rejected, so a skill cannot read outside its folder.

## The `Skill` trait

```rust
pub trait Skill: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn content(&self) -> &str;
    fn category(&self) -> Option<&str>;
    fn tags(&self) -> &[String];
    fn read(&self, path: &str) -> Option<&str>; // bundled file by relative path
    fn files(&self) -> &[String];               // bundled file list
}
```

`MarkdownSkill` is the only implementation; there are no per-skill Rust
modules.

## Concrete skills

| Skill folder | Category |
|---|---|
| `crud_creation` (has `docs/`) | `development` |
| `analytics_and_reporting` | `analytics` |
| `customization_branding` | `customization` |
| `notification_creation` | `notifications` |
| `profile_management` | `profile` |
| `roles_and_permissions` | `security` |
| `qa_testing` | `quality_assurance` |

## How to add or change a runtime skill

1. Create `src-tauri/src/skills/<skill_name>/SKILL.md` with frontmatter.
2. (Optional) add supporting files, e.g. `docs/my_topic.md`, and link them from
   `SKILL.md` so the agent knows when to load them.
3. Nothing else — the skill is discovered and embedded automatically on the
   next build. There is no registry, `match` statement, or Rust struct to edit.

To load a supporting file from a skill:

```javascript
load_skill({ skillName: "crud_creation", path: "docs/row_actions.md" })
```

## UI touchpoint

The desktop chat UI renders a friendly label for the tool call; the `load_skill`
formatter lives in `src/Chat.tsx` and shows the path when present.
