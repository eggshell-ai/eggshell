//! `list_directory` — a compact, indented tree of a shell's `src/` directory.
//!
//! The output is deliberately **not** JSON: models read indented trees well and
//! they cost far fewer tokens than a structured listing. Ordering is fully
//! deterministic (directories first, then files, both alphabetical) so repeated
//! calls produce byte-identical output, which keeps prompt caching effective.
//!
//! Two optimisations keep the listing small without hiding anything important:
//!
//! * **Collapsed directories.** A handful of directories are almost never
//!   relevant to the agent (`vendor`, `node_modules`, build output, ...). They
//!   are rendered as a single line with an item count instead of being walked.
//! * **Hard entry cap.** At most [`MAX_ENTRIES`] lines are emitted; the rest is
//!   summarised in one truncation marker.

use async_trait::async_trait;
use serde_json::{json, Map, Value};
use std::fs;
use std::path::Path;

use crate::llm::{LlmResult, Tool};

use super::paths::resolve_shell_path;

/// The most lines a listing will ever render.
pub(crate) const MAX_ENTRIES: usize = 200;

/// Safety valve for the post-truncation counting pass. Without it a
/// pathological tree could make the tool walk for a very long time just to
/// report an exact "N more" number.
const MAX_COUNTED: usize = 50_000;

/// Directory names that are never worth walking.
const COLLAPSED_NAMES: &[&str] = &[
    "vendor",
    "node_modules",
    ".git",
    "dist",
    "build",
    ".next",
    ".nuxt",
    "coverage",
];

/// Multi-segment paths (relative to the shell's `src/`) that are never worth
/// walking. Symfony writes its cache, logs and compiled assets here.
const COLLAPSED_PATHS: &[&str] = &[
    "var/cache",
    "var/log",
    "public/build",
    "public/bundles",
];

/// The rendered result of a directory walk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Listing {
    pub tree: String,
    pub truncated: bool,
    /// Entries rendered, not counting the truncation marker.
    pub shown: usize,
    /// Entries that exist but were not rendered.
    pub hidden: usize,
}

impl Listing {
    /// The tree text plus a truncation marker, ready to return to the model.
    pub fn to_text(&self) -> String {
        if !self.truncated {
            return self.tree.clone();
        }
        let marker = format!(
            "... {} more entries not shown (use a deeper path or a smaller depth)",
            format_count(self.hidden)
        );
        if self.tree.is_empty() {
            marker
        } else {
            format!("{}\n{}", self.tree, marker)
        }
    }
}

/// Walks `root` and renders an indented tree, stopping after `max` entries.
pub(crate) fn render_tree(
    root: &Path,
    depth: usize,
    include_hidden: bool,
    max: usize,
) -> Listing {
    let mut walk = Walk {
        lines: Vec::new(),
        shown: 0,
        hidden: 0,
        counted: 0,
        max,
        truncated: false,
        include_hidden,
        depth: depth.max(1),
    };
    walk.visit(root, "", 1);
    Listing {
        tree: walk.lines.join("\n"),
        truncated: walk.truncated,
        shown: walk.shown,
        hidden: walk.hidden,
    }
}

struct Walk {
    lines: Vec<String>,
    shown: usize,
    hidden: usize,
    counted: usize,
    max: usize,
    truncated: bool,
    include_hidden: bool,
    depth: usize,
}

impl Walk {
    /// Renders the children of `dir`. `relative` is the path from the walked
    /// root (used for multi-segment collapse rules).
    fn visit(&mut self, dir: &Path, relative: &str, level: usize) {
        // Once the cap is reached the tree is only being counted, and past the
        // counting budget even that stops, so a pathological tree cannot make
        // the call walk for a long time.
        if self.truncated && self.counted >= MAX_COUNTED {
            return;
        }
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(_) => {
                self.push_line(format!("{}/  [permission denied]", name_of(dir)));
                return;
            }
        };

        let mut dirs: Vec<SortEntry> = Vec::new();
        let mut files: Vec<SortEntry> = Vec::new();

        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !self.include_hidden && name.starts_with('.') {
                continue;
            }
            // `file_type()` does not follow symlinks, so a link to a directory
            // is rendered as a file and never recursed into.
            let is_dir = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
            let child_relative = if relative.is_empty() {
                name.clone()
            } else {
                format!("{relative}/{name}")
            };
            let entry = SortEntry {
                name,
                is_dir,
                relative: child_relative,
            };
            if is_dir {
                dirs.push(entry);
            } else {
                files.push(entry);
            }
        }

        dirs.sort_by(|a, b| compare_names(&a.name, &b.name));
        files.sort_by(|a, b| compare_names(&a.name, &b.name));

        for entry in dirs.into_iter().chain(files) {
            if entry.is_dir {
                self.visit_dir(dir, &entry, level);
            } else {
                self.push_line(format!("{}{}", indent(level), entry.name));
            }
        }
    }

    fn visit_dir(&mut self, parent: &Path, entry: &SortEntry, level: usize) {
        let prefix = indent(level);

        if is_collapsed(&entry.name, &entry.relative) {
            let count = fs::read_dir(parent.join(&entry.name))
                .map(|children| children.flatten().count())
                .unwrap_or(0);
            self.push_line(format!(
                "{}{}/  [collapsed: {} items]",
                prefix,
                entry.name,
                format_count(count)
            ));
            return;
        }

        self.push_line(format!("{}{}/", prefix, entry.name));

        if level >= self.depth {
            return;
        }
        self.visit(&parent.join(&entry.name), &entry.relative, level + 1);
    }

    fn push_line(&mut self, line: String) {
        if self.shown >= self.max {
            // Over the cap: count instead of rendering so the truncation marker
            // can report how much is missing.
            self.truncated = true;
            self.count_one();
            return;
        }
        self.shown += 1;
        self.lines.push(line);
    }

    /// Counts one entry, and (for directories we would have walked) its
    /// contents, respecting the same depth and collapse rules.
    fn count_one(&mut self) {
        if self.counted >= MAX_COUNTED {
            return;
        }
        self.counted += 1;
        self.hidden += 1;
    }
}

struct SortEntry {
    name: String,
    is_dir: bool,
    relative: String,
}

fn is_collapsed(name: &str, relative: &str) -> bool {
    COLLAPSED_NAMES.contains(&name) || COLLAPSED_PATHS.contains(&relative)
}

fn indent(level: usize) -> String {
    "  ".repeat(level.saturating_sub(1))
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

/// Case-insensitive alphabetical order with a deterministic tiebreak.
fn compare_names(left: &str, right: &str) -> std::cmp::Ordering {
    left.to_lowercase()
        .cmp(&right.to_lowercase())
        .then_with(|| left.cmp(right))
}

/// `2481` -> `2,481`, matching the shape used in the tool description.
pub(crate) fn format_count(value: usize) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(character);
    }
    out
}

pub struct ListDirectoryTool {
    parameters: Map<String, Value>,
}

impl ListDirectoryTool {
    pub fn new() -> Self {
        Self {
            parameters: json!({
                "type": "object",
                "properties": {
                    "shell": {
                        "type": "string",
                        "enum": ["frontend", "backend"],
                        "description": "Target shell directory (frontend or backend)."
                    },
                    "path": {
                        "type": "string",
                        "description": "Relative directory path inside the src directory. Omit or leave empty to list the src root."
                    },
                    "depth": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 6,
                        "description": "How many levels to descend. 1 lists only the immediate contents. Defaults to 2."
                    },
                    "include_hidden": {
                        "type": "boolean",
                        "description": "Include dotfiles and dot-directories. Defaults to false."
                    },
                    "projectPath": {
                        "type": "string",
                        "description": "Overrides the target project directory."
                    }
                },
                "required": ["shell"]
            })
            .as_object()
            .expect("list_directory parameters must be an object")
            .clone(),
        }
    }
}

impl Default for ListDirectoryTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ListDirectoryTool {
    fn name(&self) -> &str {
        "list_directory"
    }

    fn description(&self) -> &str {
        "Lists a directory inside a shell's src as a compact indented tree (directories first, then files, alphabetical). Use this instead of guessing file paths. Dependency and build directories (vendor, node_modules, .git, dist, build, .next, .nuxt, coverage, var/cache, var/log, public/build, public/bundles) are collapsed into a single line with an item count. Output is capped at 200 entries; a truncation marker tells you how many were omitted."
    }

    fn parameters(&self) -> &Map<String, Value> {
        &self.parameters
    }

    async fn execute(&self, args: Value) -> LlmResult<Value> {
        let resolved = resolve_shell_path(&args, true)?;

        let depth = args
            .get("depth")
            .and_then(Value::as_u64)
            .map(|value| value.clamp(1, 6) as usize)
            .unwrap_or(2);
        let include_hidden = args
            .get("include_hidden")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        if !resolved.target.exists() {
            return Ok(json!({
                "success": false,
                "message": format!("Directory not found at path: {}", resolved.target.display())
            }));
        }
        if !resolved.target.is_dir() {
            return Ok(json!({
                "success": false,
                "message": format!(
                    "Path is a file, not a directory: {}. Use read_file instead.",
                    resolved.target.display()
                )
            }));
        }

        let listing = render_tree(&resolved.target, depth, include_hidden, MAX_ENTRIES);
        let tree = listing.to_text();
        let message = if listing.truncated {
            format!(
                "Listed {} entries; {} more were omitted.",
                format_count(listing.shown),
                format_count(listing.hidden)
            )
        } else {
            format!("Listed {} entries.", format_count(listing.shown))
        };

        Ok(json!({
            "success": true,
            "message": message,
            "details": {
                "shell": resolved.shell,
                "path": resolved.relative,
                "filePath": resolved.target,
                "tree": tree,
                "truncated": listing.truncated,
                "shown": listing.shown,
                "hidden": listing.hidden
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Builds a throwaway tree under the OS temp directory.
    fn fixture(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "eggshell-list-{name}-{}",
            uuid::Uuid::new_v4()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create fixture root");
        root
    }

    fn write(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(path, contents).expect("write fixture file");
    }

    #[test]
    fn directories_sort_before_files_alphabetically() {
        let root = fixture("order");
        write(&root.join("zebra.txt"), "z");
        write(&root.join("Alpha.txt"), "a");
        fs::create_dir_all(root.join("Menus")).expect("dir");
        fs::create_dir_all(root.join("app")).expect("dir");

        let listing = render_tree(&root, 1, false, MAX_ENTRIES);
        let lines: Vec<&str> = listing.tree.lines().collect();
        assert_eq!(lines, vec!["app/", "Menus/", "Alpha.txt", "zebra.txt"]);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn depth_one_lists_only_immediate_contents() {
        let root = fixture("depth");
        write(&root.join("app/(dashboard)/page.tsx"), "page");

        let shallow = render_tree(&root, 1, false, MAX_ENTRIES);
        assert_eq!(shallow.tree, "app/");

        let deep = render_tree(&root, 3, false, MAX_ENTRIES);
        assert_eq!(deep.tree, "app/\n  (dashboard)/\n    page.tsx");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn collapsed_directories_are_counted_not_walked() {
        let root = fixture("collapse");
        for index in 0..3 {
            write(&root.join(format!("vendor/pkg{index}/index.php")), "<?php");
        }
        write(&root.join("README.md"), "readme");

        let listing = render_tree(&root, 3, false, MAX_ENTRIES);
        assert_eq!(listing.tree, "vendor/  [collapsed: 3 items]\nREADME.md");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn symfony_asset_and_cache_paths_collapse() {
        let root = fixture("symfony");
        write(&root.join("public/build/app.js"), "js");
        write(&root.join("var/cache/dev/pool"), "cache");
        write(&root.join("src/Controller.php"), "<?php");

        let listing = render_tree(&root, 4, false, MAX_ENTRIES);
        assert!(listing.tree.contains("build/  [collapsed: 1 items]"));
        assert!(listing.tree.contains("cache/  [collapsed: 1 items]"));
        assert!(!listing.tree.contains("app.js"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn hidden_entries_are_skipped_unless_requested() {
        let root = fixture("hidden");
        write(&root.join(".env"), "secret");
        write(&root.join(".next/trace"), "trace");
        write(&root.join("visible.txt"), "hi");

        let hidden = render_tree(&root, 2, false, MAX_ENTRIES);
        assert_eq!(hidden.tree, "visible.txt");

        let shown = render_tree(&root, 2, true, MAX_ENTRIES);
        assert!(shown.tree.contains(".next/  [collapsed: 1 items]"));
        assert!(shown.tree.contains(".env"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn output_is_capped_with_a_truncation_marker() {
        let root = fixture("truncate");
        for index in 0..10 {
            write(&root.join(format!("file{index:02}.txt")), "x");
        }

        let listing = render_tree(&root, 1, false, 4);
        assert!(listing.truncated);
        assert_eq!(listing.shown, 4);
        assert_eq!(listing.hidden, 6);
        let text = listing.to_text();
        assert!(text.ends_with("... 6 more entries not shown (use a deeper path or a smaller depth)"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn counts_are_grouped_with_commas() {
        assert_eq!(format_count(0), "0");
        assert_eq!(format_count(999), "999");
        assert_eq!(format_count(2481), "2,481");
        assert_eq!(format_count(1200000), "1,200,000");
    }

    #[tokio::test]
    async fn tool_rejects_a_file_path() {
        let root = fixture("isfile");
        write(&root.join("backend/src/app.py"), "print()");

        let tool = ListDirectoryTool::new();
        let result = tool
            .execute(json!({
                "shell": "backend",
                "path": "app.py",
                "projectPath": root.to_string_lossy()
            }))
            .await
            .expect("tool executes");

        assert_eq!(result["success"], false);
        assert!(result["message"].as_str().unwrap().contains("read_file"));

        let _ = fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn tool_lists_the_src_root_when_path_is_omitted() {
        let root = fixture("root");
        write(&root.join("frontend/src/menu-items/menu.json"), "[]");

        let tool = ListDirectoryTool::new();
        let result = tool
            .execute(json!({ "shell": "frontend", "depth": 1, "projectPath": root.to_string_lossy() }))
            .await
            .expect("tool executes");

        assert_eq!(result["success"], true);
        assert_eq!(result["details"]["tree"], "menu-items/");

        let _ = fs::remove_dir_all(&root);
    }
}
