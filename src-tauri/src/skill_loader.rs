//! Discovers, parses, and embeds the bundled skills.
//!
//! Skills follow the folder standard: every skill lives in its own directory
//! under `src/skills/` and keeps its instructions in a `SKILL.md` file. Any
//! other file inside that directory (typically under `docs/`) is bundled too,
//! so a skill can reference supporting material without declaring it anywhere:
//! the agent reads it by path through the `load_skill` tool.
//!
//! Skill metadata (`name`, `description`, `category`, `tags`) comes from the
//! YAML frontmatter of `SKILL.md`. The whole tree is embedded at compile time
//! with [`include_dir!`], so there is no runtime filesystem access.

use std::collections::BTreeMap;

use include_dir::{include_dir, Dir};
use serde::Deserialize;

use crate::llm::Skill;

/// The complete `src/skills` tree, embedded recursively at compile time.
static SKILLS_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/src/skills");

/// The instructions file every skill directory must contain.
const SKILL_FILE: &str = "SKILL.md";

/// YAML frontmatter understood by [`SKILL.md`] files.
#[derive(Debug, Default, Deserialize)]
struct Frontmatter {
    name: Option<String>,
    description: Option<String>,
    category: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
}

/// A skill backed entirely by a `SKILL.md` file and its sibling documents.
pub struct MarkdownSkill {
    name: String,
    description: String,
    category: Option<String>,
    tags: Vec<String>,
    /// Relative directory name, e.g. `crud_creation`.
    directory: String,
    /// Instructions body (frontmatter stripped).
    content: &'static str,
    /// Every bundled file in the skill directory, relative to it and sorted.
    files: Vec<String>,
}

impl MarkdownSkill {
    /// Builds a skill from the directory named `directory`, returning `None`
    /// when that directory has no `SKILL.md` or when it is not valid UTF-8.
    fn from_dir(directory: &str, dir: &'static Dir<'static>) -> Option<Self> {
        let raw = SKILLS_DIR
            .get_file(format!("{directory}/{SKILL_FILE}"))?
            .contents_utf8()?;

        let (frontmatter, body) = split_frontmatter(raw);
        let metadata = frontmatter
            .and_then(|yaml| serde_yaml::from_str::<Frontmatter>(yaml).ok())
            .unwrap_or_default();

        let name = metadata
            .name
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| directory.to_owned());
        let description = metadata
            .description
            .filter(|description| !description.trim().is_empty())
            .unwrap_or_else(|| first_heading(body).unwrap_or_default());

        Some(Self {
            name,
            description,
            category: metadata
                .category
                .filter(|category| !category.trim().is_empty()),
            tags: metadata.tags,
            files: list_files(dir),
            directory: directory.to_owned(),
            content: body,
        })
    }

    /// Resolves `path` relative to the skill directory.
    fn resolve(&self, path: &str) -> Option<String> {
        let path = path.trim().replace('\\', "/");
        if path.is_empty() || path.starts_with('/') || path.contains(':') {
            return None;
        }
        if path.split('/').any(|segment| segment == ".." || segment == ".") {
            return None;
        }
        Some(format!("{}/{path}", self.directory))
    }
}

impl Skill for MarkdownSkill {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn content(&self) -> &str {
        self.content
    }

    fn category(&self) -> Option<&str> {
        self.category.as_deref()
    }

    fn tags(&self) -> &[String] {
        &self.tags
    }

    fn read(&self, path: &str) -> Option<&str> {
        let path = self.resolve(path)?;
        SKILLS_DIR.get_file(path)?.contents_utf8()
    }

    fn files(&self) -> &[String] {
        &self.files
    }
}

/// Returns every skill found under `src/skills/`, sorted by name.
///
/// Directories without a `SKILL.md` are ignored, which keeps unrelated files
/// (such as a shared `docs/` folder) from being mistaken for skills.
pub fn load_all() -> Vec<Box<dyn Skill>> {
    let mut skills: BTreeMap<String, Box<dyn Skill>> = BTreeMap::new();

    for dir in SKILLS_DIR.dirs() {
        let Some(directory) = dir.path().file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(skill) = MarkdownSkill::from_dir(directory, dir) else {
            continue;
        };
        skills.insert(skill.name().to_owned(), Box::new(skill));
    }

    skills.into_values().collect()
}

/// Splits a `SKILL.md` into its YAML frontmatter and body.
///
/// Returns `(None, raw)` when the file has no frontmatter block. The body has
/// leading blank lines removed, but is otherwise returned verbatim.
fn split_frontmatter(raw: &str) -> (Option<&str>, &str) {
    let text = raw.strip_prefix('\u{feff}').unwrap_or(raw);

    let Some(rest) = text.strip_prefix("---") else {
        return (None, raw);
    };
    let Some(rest) = rest
        .strip_prefix("\r\n")
        .or_else(|| rest.strip_prefix('\n'))
    else {
        return (None, raw);
    };

    let mut offset = 0usize;
    for line in rest.split_inclusive('\n') {
        if line.trim() == "---" {
            let yaml = &rest[..offset];
            let body = &rest[offset + line.len()..];
            return (Some(yaml), body.trim_start_matches(['\r', '\n']));
        }
        offset += line.len();
    }

    (None, raw)
}

/// Extracts the first `# Heading` line, used when frontmatter omits a
/// description.
fn first_heading(body: &str) -> Option<String> {
    body.lines()
        .find_map(|line| line.trim().strip_prefix("# "))
        .map(|heading| heading.trim().to_owned())
}

/// Lists every file in `dir`, relative to it, sorted for deterministic output.
fn list_files(dir: &Dir<'static>) -> Vec<String> {
    fn walk(dir: &Dir<'static>, prefix: &str, files: &mut Vec<String>) {
        for file in dir.files() {
            let name = file.path().file_name().and_then(|name| name.to_str());
            if let Some(name) = name {
                files.push(format!("{prefix}{name}"));
            }
        }
        for child in dir.dirs() {
            let Some(name) = child.path().file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            walk(child, &format!("{prefix}{name}/"), files);
        }
    }

    let mut files = Vec::new();
    walk(dir, "", &mut files);
    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_skill_directory_exposes_a_skill() {
        let skills = load_all();
        assert!(skills.len() >= 7, "expected the bundled skills to load");

        for skill in &skills {
            assert!(
                !skill.name().trim().is_empty(),
                "{} has no name",
                skill.name()
            );
            assert!(
                !skill.description().trim().is_empty(),
                "{} has no description",
                skill.name()
            );
            assert!(
                skill.files().iter().any(|file| file == SKILL_FILE),
                "{} does not list {SKILL_FILE}",
                skill.name()
            );
        }
    }

    #[test]
    fn skill_names_match_their_directory() {
        for skill in load_all() {
            assert!(
                skill.files().iter().any(|file| file == SKILL_FILE),
                "{} has no {SKILL_FILE}",
                skill.name()
            );
            assert!(
                skill.content().trim_start().starts_with('#'),
                "{} does not start with a heading (frontmatter not stripped?)",
                skill.name()
            );
        }
    }

    #[test]
    fn frontmatter_metadata_is_parsed() {
        let skills = load_all();
        let crud = skills
            .iter()
            .find(|skill| skill.name() == "crud_creation")
            .expect("crud_creation is bundled");

        assert_eq!(crud.category(), Some("development"));
        assert!(crud.tags().iter().any(|tag| tag == "crud"));
        assert!(!crud.content().contains("name: crud_creation"));
    }

    #[test]
    fn documents_are_readable_by_path() {
        let skills = load_all();
        let crud = skills
            .iter()
            .find(|skill| skill.name() == "crud_creation")
            .expect("crud_creation is bundled");

        assert!(crud.files().iter().any(|file| file == "docs/row_actions.md"));
        let content = crud.read("docs/row_actions.md").expect("document is bundled");
        assert!(content.contains("Custom Row Actions"));
    }

    #[test]
    fn paths_cannot_escape_the_skill_directory() {
        let skills = load_all();
        let crud = skills
            .iter()
            .find(|skill| skill.name() == "crud_creation")
            .expect("crud_creation is bundled");

        assert!(crud.read("../Cargo.toml").is_none());
        assert!(crud.read("/Cargo.toml").is_none());
        assert!(crud.read("docs/../../Cargo.toml").is_none());
        assert!(crud.read("").is_none());
    }

    #[test]
    fn frontmatter_parsing_handles_missing_and_present_blocks() {
        let (yaml, body) = split_frontmatter("---\nname: x\n---\n\n# Title\n");
        assert_eq!(yaml, Some("name: x\n"));
        assert_eq!(body, "# Title\n");

        let (yaml, body) = split_frontmatter("# Title\n");
        assert!(yaml.is_none());
        assert_eq!(body, "# Title\n");
    }
}
