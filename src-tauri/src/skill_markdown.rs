//! Loads skill instruction markdown that is bundled into the binary.
//!
//! Each skill keeps its instructions in a dedicated `.md` file under
//! `src/skills/` and only stores that file's name. This module is the single
//! reusable place that maps a markdown file name to its contents, so the
//! loading code is not duplicated across the individual skill modules.
//!
//! The markdown is embedded at compile time with [`include_str!`], so there is
//! no runtime filesystem access and the content is always available.

/// Loads the markdown instructions for the given skill markdown file name.
///
/// `name` is the file name of a `.md` file stored under `src/skills/`, for
/// example `"crud_creation.md"`.
pub fn load(name: &str) -> &'static str {
    match name {
        "crud_creation.md" => include_str!("skills/crud_creation.md"),
        "analytics_and_reporting.md" => include_str!("skills/analytics_and_reporting.md"),
        other => panic!("No skill markdown file named '{other}'"),
    }
}
