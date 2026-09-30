use async_trait::async_trait;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashSet};
use std::path::{Component, Path, PathBuf};

use crate::llm::{LlmResult, Tool};

pub struct PatchFileTool {
    parameters: Map<String, Value>,
}

impl PatchFileTool {
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
                    "patch": {
                        "type": "string",
                        "description": "The patch content following OpenAI apply_patch format starting with '*** Begin Patch' and ending with '*** End Patch'."
                    },
                    "projectPath": {
                        "type": "string",
                        "description": "Overrides the target project directory."
                    }
                },
                "required": ["shell", "patch"]
            })
            .as_object()
            .expect("patch_file parameters must be an object")
            .clone(),
        }
    }
}

impl Default for PatchFileTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for PatchFileTool {
    fn name(&self) -> &str {
        "patch_file"
    }

    fn description(&self) -> &str {
        "Applies OpenAI apply_patch format to files in either the frontend or backend shell within src directory."
    }

    fn parameters(&self) -> &Map<String, Value> {
        &self.parameters
    }

    async fn execute(&self, args: Value) -> LlmResult<Value> {
        let shell = args
            .get("shell")
            .and_then(Value::as_str)
            .ok_or_else(|| "shell is required".to_string())?;
        if !matches!(shell, "frontend" | "backend") {
            return Err("shell must be either 'frontend' or 'backend'".into());
        }

        let patch_text = args
            .get("patch")
            .and_then(Value::as_str)
            .ok_or_else(|| "patch is required and must be a string".to_string())?;

        let project_path = args
            .get("projectPath")
            .and_then(Value::as_str)
            .filter(|path| !path.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("content").join("dummy-project"));

        let base_dir = project_path.join(shell).join("src");

        // Helper to validate and resolve paths relative to base_dir
        let resolve_safe_path = |rel_path: &str| -> Result<PathBuf, String> {
            let p = Path::new(rel_path.trim());
            if p.as_os_str().is_empty() {
                return Err("File path in patch cannot be empty".to_string());
            }
            if p.is_absolute()
                || p.components().any(|component| {
                    matches!(
                        component,
                        Component::ParentDir | Component::RootDir | Component::Prefix(_)
                    )
                })
            {
                return Err(format!(
                    "Path '{}' must remain within the shell src directory",
                    rel_path
                ));
            }
            Ok(base_dir.join(p))
        };

        // I/O closures for process_patch
        let open_fn = |path: &str| -> Result<String, String> {
            let target = resolve_safe_path(path)?;
            if !target.exists() {
                return Err(format!("Missing File: {}", path));
            }
            std::fs::read_to_string(&target)
                .map_err(|e| format!("Failed to read '{}': {}", path, e))
        };

        let write_fn = |path: &str, content: &str| -> Result<(), String> {
            let target = resolve_safe_path(path)?;
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create directories for '{}': {}", path, e))?;
            }
            std::fs::write(&target, content)
                .map_err(|e| format!("Failed to write '{}': {}", path, e))
        };

        let remove_fn = |path: &str| -> Result<(), String> {
            let target = resolve_safe_path(path)?;
            if target.exists() {
                std::fs::remove_file(&target)
                    .map_err(|e| format!("Failed to delete '{}': {}", path, e))?;
            }
            Ok(())
        };

        match process_patch(patch_text, open_fn, write_fn, remove_fn) {
            Ok((msg, fuzz, commit)) => {
                let modified_files: Vec<String> = commit.changes.keys().cloned().collect();
                Ok(json!({
                    "success": true,
                    "message": msg,
                    "fuzz": fuzz,
                    "details": {
                        "shell": shell,
                        "baseDir": base_dir,
                        "files": modified_files,
                        "timestamp": format_timestamp()
                    }
                }))
            }
            Err(err) => Ok(json!({
                "success": false,
                "error": err
            })),
        }
    }
}

fn format_timestamp() -> String {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => duration.as_secs().to_string(),
        Err(_) => "0".to_string(),
    }
}

// ---------------------------------------------------------
// OpenAI apply_patch parser and applier logic
// ---------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionType {
    Add,
    Delete,
    Update,
}

#[derive(Debug, Clone)]
pub struct FileChange {
    pub action_type: ActionType,
    #[allow(dead_code)]
    pub old_content: Option<String>,
    pub new_content: Option<String>,
    pub move_path: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Commit {
    pub changes: BTreeMap<String, FileChange>,
}

#[derive(Debug, Clone, Default)]
pub struct Chunk {
    pub orig_index: isize,
    pub del_lines: Vec<String>,
    pub ins_lines: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PatchAction {
    pub action_type: ActionType,
    pub new_file: Option<String>,
    pub chunks: Vec<Chunk>,
    pub move_path: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Patch {
    pub actions: BTreeMap<String, PatchAction>,
}

struct Parser<'a> {
    current_files: &'a BTreeMap<String, String>,
    lines: Vec<&'a str>,
    index: usize,
    patch: Patch,
    fuzz: usize,
}

impl<'a> Parser<'a> {
    fn new(lines: Vec<&'a str>, current_files: &'a BTreeMap<String, String>) -> Self {
        Self {
            current_files,
            lines,
            index: 0,
            patch: Patch::default(),
            fuzz: 0,
        }
    }

    fn is_done(&self, prefixes: &[&str]) -> bool {
        if self.index >= self.lines.len() {
            return true;
        }
        let cur = self.lines[self.index];
        prefixes.iter().any(|prefix| cur.starts_with(prefix))
    }

    fn startswith(&self, prefixes: &[&str]) -> Result<bool, String> {
        if self.index >= self.lines.len() {
            return Err(format!("Unexpected end of patch at index {}", self.index));
        }
        let cur = self.lines[self.index];
        Ok(prefixes.iter().any(|p| cur.starts_with(p)))
    }

    fn read_str(&mut self, prefix: &str, return_everything: bool) -> Result<String, String> {
        if self.index >= self.lines.len() {
            return Err(format!("Unexpected end of patch at index {}", self.index));
        }
        let line = self.lines[self.index];
        if line.starts_with(prefix) {
            let text = if return_everything {
                line.to_string()
            } else {
                line[prefix.len()..].to_string()
            };
            self.index += 1;
            Ok(text)
        } else {
            Ok(String::new())
        }
    }

    fn parse(&mut self) -> Result<(), String> {
        while !self.is_done(&["*** End Patch"]) {
            let path = self.read_str("*** Update File: ", false)?;
            if !path.is_empty() {
                if self.patch.actions.contains_key(&path) {
                    return Err(format!("Update File Error: Duplicate Path: {}", path));
                }
                let move_to = self.read_str("*** Move to: ", false)?;
                if !move_to.is_empty() {
                    let parts: Vec<&str> = move_to.split('/').collect();
                    if parts.contains(&"..") || move_to.starts_with('/') {
                        return Err(format!(
                            "Update File Error: Invalid move path '{}': must be a relative path without '..' components",
                            move_to
                        ));
                    }
                }
                let text = self
                    .current_files
                    .get(&path)
                    .ok_or_else(|| format!("Update File Error: Missing File: {}", path))?;
                let mut action = self.parse_update_file(text)?;
                action.move_path = if move_to.is_empty() {
                    None
                } else {
                    Some(move_to)
                };
                self.patch.actions.insert(path, action);
                continue;
            }

            let path = self.read_str("*** Delete File: ", false)?;
            if !path.is_empty() {
                if self.patch.actions.contains_key(&path) {
                    return Err(format!("Delete File Error: Duplicate Path: {}", path));
                }
                if !self.current_files.contains_key(&path) {
                    return Err(format!("Delete File Error: Missing File: {}", path));
                }
                self.patch.actions.insert(
                    path,
                    PatchAction {
                        action_type: ActionType::Delete,
                        new_file: None,
                        chunks: Vec::new(),
                        move_path: None,
                    },
                );
                continue;
            }

            let path = self.read_str("*** Add File: ", false)?;
            if !path.is_empty() {
                if self.patch.actions.contains_key(&path) {
                    return Err(format!("Add File Error: Duplicate Path: {}", path));
                }
                let action = self.parse_add_file()?;
                self.patch.actions.insert(path, action);
                continue;
            }

            return Err(format!("Unknown Line: {}", self.lines[self.index]));
        }

        if !self.startswith(&["*** End Patch"])? {
            return Err("Missing End Patch".to_string());
        }
        self.index += 1;
        Ok(())
    }

    fn parse_update_file(&mut self, text: &str) -> Result<PatchAction, String> {
        let mut action = PatchAction {
            action_type: ActionType::Update,
            new_file: None,
            chunks: Vec::new(),
            move_path: None,
        };
        let lines: Vec<&str> = text.split('\n').collect();
        let mut index = 0usize;

        while !self.is_done(&[
            "*** End Patch",
            "*** Update File:",
            "*** Delete File:",
            "*** Add File:",
            "*** End of File",
        ]) {
            let def_str = self.read_str("@@ ", false)?;
            let mut section_str = String::new();
            if def_str.is_empty() && self.index < self.lines.len() && self.lines[self.index] == "@@" {
                section_str = self.lines[self.index].to_string();
                self.index += 1;
            }
            if def_str.is_empty() && section_str.is_empty() && index != 0 {
                return Err(format!("Invalid Line:\n{}", self.lines[self.index]));
            }

            if !def_str.trim().is_empty() {
                let mut found = false;
                let before_matches: bool = lines[..index].iter().any(|s| *s == def_str);
                if !before_matches {
                    for (i, s) in lines.iter().enumerate().skip(index) {
                        if *s == def_str {
                            index = i + 1;
                            found = true;
                            break;
                        }
                    }
                }
                let trimmed_before_matches: bool =
                    lines[..index].iter().any(|s| s.trim() == def_str.trim());
                if !found && !trimmed_before_matches {
                    for (i, s) in lines.iter().enumerate().skip(index) {
                        if s.trim() == def_str.trim() {
                            index = i + 1;
                            self.fuzz += 1;
                            break;
                        }
                    }
                }
            }

            let (next_chunk_context, chunks, end_patch_index, eof) =
                peek_next_section(&self.lines, self.index)?;
            let next_chunk_text = next_chunk_context.join("\n");
            let (new_index, fuzz) = find_context(&lines, &next_chunk_context, index, eof)?;
            if new_index == -1 {
                if eof {
                    return Err(format!("Invalid EOF Context {}:\n{}", index, next_chunk_text));
                } else {
                    return Err(format!("Invalid Context {}:\n{}", index, next_chunk_text));
                }
            }
            self.fuzz += fuzz;
            for mut ch in chunks {
                ch.orig_index += new_index as isize;
                action.chunks.push(ch);
            }
            index = (new_index as usize) + next_chunk_context.len();
            self.index = end_patch_index;
        }

        Ok(action)
    }

    fn parse_add_file(&mut self) -> Result<PatchAction, String> {
        let mut lines = Vec::new();
        while !self.is_done(&[
            "*** End Patch",
            "*** Update File:",
            "*** Delete File:",
            "*** Add File:",
        ]) {
            let s = self.read_str("", true)?;
            if !s.starts_with('+') {
                return Err(format!("Invalid Add File Line: {}", s));
            }
            lines.push(s[1..].to_string());
        }
        Ok(PatchAction {
            action_type: ActionType::Add,
            new_file: Some(lines.join("\n")),
            chunks: Vec::new(),
            move_path: None,
        })
    }
}

fn find_context_core(lines: &[&str], context: &[&str], start: usize) -> (isize, usize) {
    if context.is_empty() {
        return (start as isize, 0);
    }
    let ctx_len = context.len();
    if lines.len() >= ctx_len {
        for i in start..=lines.len() - ctx_len {
            if lines[i..i + ctx_len] == context[..] {
                return (i as isize, 0);
            }
        }
        for i in start..=lines.len() - ctx_len {
            let matched = lines[i..i + ctx_len]
                .iter()
                .zip(context.iter())
                .all(|(a, b)| a.trim_end() == b.trim_end());
            if matched {
                return (i as isize, 1);
            }
        }
        for i in start..=lines.len() - ctx_len {
            let matched = lines[i..i + ctx_len]
                .iter()
                .zip(context.iter())
                .all(|(a, b)| a.trim() == b.trim());
            if matched {
                return (i as isize, 100);
            }
        }
    }
    (-1, 0)
}

fn find_context(
    lines: &[&str],
    context: &[&str],
    start: usize,
    eof: bool,
) -> Result<(isize, usize), String> {
    if eof {
        let eof_start = if lines.len() >= context.len() {
            lines.len() - context.len()
        } else {
            0
        };
        let (new_index, fuzz) = find_context_core(lines, context, eof_start);
        if new_index != -1 {
            return Ok((new_index, fuzz));
        }
        let (new_index, fuzz) = find_context_core(lines, context, start);
        return Ok((new_index, fuzz + 10000));
    }
    Ok(find_context_core(lines, context, start))
}

fn peek_next_section<'a>(
    lines: &[&'a str],
    mut index: usize,
) -> Result<(Vec<&'a str>, Vec<Chunk>, usize, bool), String> {
    let mut old: Vec<&'a str> = Vec::new();
    let mut del_lines: Vec<String> = Vec::new();
    let mut ins_lines: Vec<String> = Vec::new();
    let mut chunks: Vec<Chunk> = Vec::new();
    let mut mode = "keep";
    let orig_index = index;

    while index < lines.len() {
        let s = lines[index];
        if s.starts_with("@@")
            || s.starts_with("*** End Patch")
            || s.starts_with("*** Update File:")
            || s.starts_with("*** Delete File:")
            || s.starts_with("*** Add File:")
            || s.starts_with("*** End of File")
        {
            break;
        }
        if s == "***" {
            break;
        } else if s.starts_with("***") {
            return Err(format!("Invalid Line: {}", s));
        }
        index += 1;
        let last_mode = mode;
        let mut line_str = s;
        if line_str.is_empty() {
            line_str = " ";
        }
        let prefix = line_str.chars().next().unwrap();
        if prefix == '+' {
            mode = "add";
        } else if prefix == '-' {
            mode = "delete";
        } else if prefix == ' ' {
            mode = "keep";
        } else {
            return Err(format!("Invalid Line: {}", s));
        }
        let content = &line_str[1..];
        if mode == "keep" && last_mode != mode {
            if !ins_lines.is_empty() || !del_lines.is_empty() {
                chunks.push(Chunk {
                    orig_index: (old.len() as isize) - (del_lines.len() as isize),
                    del_lines: std::mem::take(&mut del_lines),
                    ins_lines: std::mem::take(&mut ins_lines),
                });
            }
        }
        if mode == "delete" {
            del_lines.push(content.to_string());
            old.push(content);
        } else if mode == "add" {
            ins_lines.push(content.to_string());
        } else if mode == "keep" {
            old.push(content);
        }
    }

    if !ins_lines.is_empty() || !del_lines.is_empty() {
        chunks.push(Chunk {
            orig_index: (old.len() as isize) - (del_lines.len() as isize),
            del_lines,
            ins_lines,
        });
    }

    if index < lines.len() && lines[index] == "*** End of File" {
        index += 1;
        return Ok((old, chunks, index, true));
    }
    if index == orig_index {
        return Err(format!(
            "Nothing in this section - index={} {}",
            index,
            lines.get(index).unwrap_or(&"")
        ));
    }

    Ok((old, chunks, index, false))
}

fn identify_files_needed(text: &str) -> Vec<String> {
    let mut result = HashSet::new();
    for line in text.lines() {
        if let Some(path) = line.strip_prefix("*** Update File: ") {
            result.insert(path.trim().to_string());
        } else if let Some(path) = line.strip_prefix("*** Delete File: ") {
            result.insert(path.trim().to_string());
        }
    }
    result.into_iter().collect()
}

fn text_to_patch(text: &str, orig: &BTreeMap<String, String>) -> Result<(Patch, usize), String> {
    let lines: Vec<&str> = text.trim().lines().collect();
    if lines.len() < 2
        || !lines.first().map(|l| l.starts_with("*** Begin Patch")).unwrap_or(false)
        || lines.last().copied() != Some("*** End Patch")
    {
        return Err("Invalid patch text: must start with '*** Begin Patch' and end with '*** End Patch'".to_string());
    }

    let mut parser = Parser::new(lines, orig);
    parser.index = 1;
    parser.parse()?;
    Ok((parser.patch, parser.fuzz))
}

fn get_updated_file(text: &str, action: &PatchAction, path: &str) -> Result<String, String> {
    if action.action_type != ActionType::Update {
        return Err(format!(
            "_get_updated_file: expected UPDATE action for '{}'",
            path
        ));
    }
    let orig_lines: Vec<&str> = text.split('\n').collect();
    let mut dest_lines: Vec<String> = Vec::new();
    let mut orig_index = 0usize;
    let mut dest_index = 0usize;

    for chunk in &action.chunks {
        let chunk_orig = chunk.orig_index as usize;
        if chunk_orig > orig_lines.len() {
            return Err(format!(
                "_get_updated_file: {}: chunk.orig_index {} > len(lines) {}",
                path,
                chunk.orig_index,
                orig_lines.len()
            ));
        }
        if orig_index > chunk_orig {
            return Err(format!(
                "_get_updated_file: {}: orig_index {} > chunk.orig_index {}",
                path, orig_index, chunk.orig_index
            ));
        }
        for line in &orig_lines[orig_index..chunk_orig] {
            dest_lines.push((*line).to_string());
        }
        let delta = chunk_orig - orig_index;
        orig_index += delta;
        dest_index += delta;

        for s in &chunk.ins_lines {
            dest_lines.push(s.clone());
        }
        dest_index += chunk.ins_lines.len();
        orig_index += chunk.del_lines.len();
    }

    for line in &orig_lines[orig_index..] {
        dest_lines.push((*line).to_string());
    }
    let delta = orig_lines.len() - orig_index;
    orig_index += delta;
    dest_index += delta;

    if orig_index != orig_lines.len() {
        return Err(format!(
            "_get_updated_file: {}: did not consume all original lines (orig_index={}, len={})",
            path,
            orig_index,
            orig_lines.len()
        ));
    }
    if dest_index != dest_lines.len() {
        return Err(format!(
            "_get_updated_file: {}: dest line count mismatch (dest_index={}, len={})",
            path,
            dest_index,
            dest_lines.len()
        ));
    }

    Ok(dest_lines.join("\n"))
}

fn patch_to_commit(patch: Patch, orig: &BTreeMap<String, String>) -> Result<Commit, String> {
    let mut commit = Commit::default();
    for (path, action) in patch.actions {
        match action.action_type {
            ActionType::Delete => {
                let old = orig.get(&path).cloned();
                commit.changes.insert(
                    path,
                    FileChange {
                        action_type: ActionType::Delete,
                        old_content: old,
                        new_content: None,
                        move_path: None,
                    },
                );
            }
            ActionType::Add => {
                commit.changes.insert(
                    path,
                    FileChange {
                        action_type: ActionType::Add,
                        old_content: None,
                        new_content: action.new_file,
                        move_path: None,
                    },
                );
            }
            ActionType::Update => {
                let old = orig
                    .get(&path)
                    .ok_or_else(|| format!("Missing original content for {}", path))?;
                let new_content = get_updated_file(old, &action, &path)?;
                commit.changes.insert(
                    path,
                    FileChange {
                        action_type: ActionType::Update,
                        old_content: Some(old.clone()),
                        new_content: Some(new_content),
                        move_path: action.move_path,
                    },
                );
            }
        }
    }
    Ok(commit)
}

fn apply_commit<W, R>(
    commit: &Commit,
    mut write_fn: W,
    mut remove_fn: R,
) -> Result<(), String>
where
    W: FnMut(&str, &str) -> Result<(), String>,
    R: FnMut(&str) -> Result<(), String>,
{
    for (path, change) in &commit.changes {
        match change.action_type {
            ActionType::Delete => {
                remove_fn(path)?;
            }
            ActionType::Add => {
                let content = change
                    .new_content
                    .as_deref()
                    .ok_or_else(|| format!("apply_commit: ADD change for '{}' has no content", path))?;
                write_fn(path, content)?;
            }
            ActionType::Update => {
                let content = change
                    .new_content
                    .as_deref()
                    .ok_or_else(|| format!("apply_commit: UPDATE change for '{}' has no content", path))?;
                if let Some(ref move_path) = change.move_path {
                    write_fn(move_path, content)?;
                    remove_fn(path)?;
                } else {
                    write_fn(path, content)?;
                }
            }
        }
    }
    Ok(())
}

pub fn process_patch<O, W, R>(
    text: &str,
    mut open_fn: O,
    write_fn: W,
    remove_fn: R,
) -> Result<(String, usize, Commit), String>
where
    O: FnMut(&str) -> Result<String, String>,
    W: FnMut(&str, &str) -> Result<(), String>,
    R: FnMut(&str) -> Result<(), String>,
{
    let trimmed = text.trim();
    if !trimmed.starts_with("*** Begin Patch") {
        return Err("Invalid patch: must start with '*** Begin Patch'".to_string());
    }
    let paths = identify_files_needed(trimmed);
    let mut orig = BTreeMap::new();
    for p in paths {
        let content = open_fn(&p)?;
        orig.insert(p, content);
    }
    let (patch, fuzz) = text_to_patch(trimmed, &orig)?;
    let commit = patch_to_commit(patch, &orig)?;
    apply_commit(&commit, write_fn, remove_fn)?;
    Ok(("Done!".to_string(), fuzz, commit))
}
