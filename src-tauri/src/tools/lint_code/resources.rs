//! Frontend resource-definition checks.
//!
//! - [`ResourceActionsCheck`] validates the `actions` array of
//!   `src/resources/*.ts`.
//!
//! Row actions are rendered by `ResourceGrid`, which only understands a fixed
//! set of properties. Anything else (a field-style `label`, a React
//! `onClick`/`icon`/`title`, ...) is silently ignored and the button simply
//! never appears — there is no runtime error to notice. This check turns that
//! silent failure into an actionable issue.
//!
//! The check parses the file with a small hand-written scanner rather than a
//! TypeScript parser: it only needs the property names of object literals that
//! sit directly inside an `actions: [...]` array, and it must not trip over
//! `label:` on resource *fields* (which is legitimate) or on `form.fields`.

use std::fs;
use std::path::{Path, PathBuf};

use super::check::{Check, CheckContext, Shell, ShellScope, Tier};
use super::frontend::resolve_frontend_dir;
use super::issue::Issue;
use crate::llm::LlmResult;

/// Every property `ResourceGrid` actually reads from a row action.
const ACTION_KEYS: [&str; 7] = [
    "permission",
    "actionExpression",
    "labelExpression",
    "label",
    "confirm",
    "handler",
    "form",
];

/// Properties that are field-level or React-level and have no meaning on an
/// action object. Called out by name because they are the common mistakes.
const FIELD_ONLY_KEYS: [&str; 6] = ["title", "icon", "onClick", "on_click", "type", "name"];

pub(crate) struct ResourceActionsCheck;

impl Check for ResourceActionsCheck {
    fn id(&self) -> &'static str {
        "resource-actions"
    }

    fn description(&self) -> &'static str {
        "Validates row-action objects in frontend `src/resources/*.ts`."
    }

    fn shells(&self) -> ShellScope {
        ShellScope::Frontend
    }

    fn tier(&self) -> Tier {
        Tier::Static
    }

    fn run(&self, ctx: &CheckContext) -> LlmResult<Vec<Issue>> {
        let mut issues = Vec::new();
        for (relative, source) in resource_files(ctx) {
            for problem in scan_actions(&source) {
                issues.push(issue_for(self.id(), &relative, &problem));
            }
        }
        Ok(issues)
    }
}

fn issue_for(source_id: &str, relative: &str, problem: &ActionProblem) -> Issue {
    Issue::error(
        Shell::Frontend,
        source_id,
        relative,
        problem.line,
        problem.message(),
    )
}

/// Collects `src/resources/*.{ts,tsx,js,jsx}` as `(relative path, contents)`.
fn resource_files(ctx: &CheckContext) -> Vec<(String, String)> {
    let frontend = resolve_frontend_dir(ctx);
    let resources_dir = frontend.join("src").join("resources");

    let mut targets: Vec<PathBuf> = Vec::new();
    match ctx.relative_path {
        Some(filter) => {
            let direct = resources_dir.join(filter);
            let from_root = frontend.join(filter);
            let chosen = if direct.exists() {
                Some(direct)
            } else if from_root.exists() {
                Some(from_root)
            } else {
                None
            };
            match chosen {
                Some(path) if path.is_dir() => push_resource_files(&path, &mut targets),
                Some(path) => targets.push(path),
                None => return Vec::new(),
            }
        }
        None => push_resource_files(&resources_dir, &mut targets),
    }

    let mut files = Vec::new();
    for path in targets {
        let Ok(source) = fs::read_to_string(&path) else {
            continue;
        };
        let relative = path
            .strip_prefix(&frontend)
            .unwrap_or(&path)
            .components()
            .map(|component| component.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        files.push((relative, source));
    }
    files
}

fn push_resource_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut found: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && is_resource_source(path))
        .collect();
    found.sort();
    out.extend(found);
}

fn is_resource_source(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("ts" | "tsx" | "js" | "jsx")
    )
}

// ---------------------------------------------------------------------------
// Scanner
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    Punct(char),
    /// A string, template literal, number, or regex: opaque to the scan.
    Literal,
}

#[derive(Debug, Clone)]
struct Token {
    tok: Tok,
    line: u64,
}

/// Scans JavaScript/TypeScript source into a flat token stream, dropping
/// whitespace and comments.
fn tokenize(source: &str) -> Vec<Token> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    let mut line = 1u64;

    while index < bytes.len() {
        let byte = bytes[index];

        match byte {
            b'\n' => {
                line += 1;
                index += 1;
            }
            b' ' | b'\t' | b'\r' => index += 1,
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index += 2;
                while index < bytes.len() {
                    if bytes[index] == b'\n' {
                        line += 1;
                    }
                    if bytes[index] == b'*' && bytes.get(index + 1) == Some(&b'/') {
                        index += 2;
                        break;
                    }
                    index += 1;
                }
            }
            b'\'' | b'"' => {
                let start_line = line;
                index = skip_quoted(bytes, index, byte, &mut line);
                tokens.push(Token {
                    tok: Tok::Literal,
                    line: start_line,
                });
            }
            b'`' => {
                let start_line = line;
                index = skip_template(bytes, index, &mut line);
                tokens.push(Token {
                    tok: Tok::Literal,
                    line: start_line,
                });
            }
            b'0'..=b'9' => {
                while index < bytes.len() && is_ident_byte(bytes[index]) {
                    index += 1;
                }
                tokens.push(Token {
                    tok: Tok::Literal,
                    line,
                });
            }
            _ if is_ident_start(byte) => {
                let start = index;
                while index < bytes.len() && is_ident_byte(bytes[index]) {
                    index += 1;
                }
                let text = source[start..index].to_string();
                tokens.push(Token {
                    tok: Tok::Ident(text),
                    line,
                });
            }
            _ => {
                // Multi-byte UTF-8 or punctuation: advance one char.
                let ch = source[index..].chars().next().unwrap_or('\0');
                index += ch.len_utf8();
                tokens.push(Token {
                    tok: Tok::Punct(ch),
                    line,
                });
            }
        }
    }

    tokens
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$'
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$' || byte == b'.'
}

fn skip_quoted(bytes: &[u8], mut index: usize, quote: u8, line: &mut u64) -> usize {
    index += 1; // opening quote
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 2,
            b'\n' => {
                *line += 1;
                index += 1;
            }
            byte if byte == quote => return index + 1,
            _ => index += 1,
        }
    }
    index
}

/// Skips a template literal, honouring `${ ... }` substitutions.
fn skip_template(bytes: &[u8], mut index: usize, line: &mut u64) -> usize {
    index += 1; // opening backtick
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 2,
            b'\n' => {
                *line += 1;
                index += 1;
            }
            b'`' => return index + 1,
            b'$' if bytes.get(index + 1) == Some(&b'{') => {
                index += 2;
                let mut depth = 1usize;
                while index < bytes.len() && depth > 0 {
                    match bytes[index] {
                        b'\n' => {
                            *line += 1;
                            index += 1;
                        }
                        b'{' => {
                            depth += 1;
                            index += 1;
                        }
                        b'}' => {
                            depth -= 1;
                            index += 1;
                        }
                        b'\'' | b'"' => {
                            index = skip_quoted(bytes, index, bytes[index], line);
                        }
                        b'`' => {
                            index = skip_template(bytes, index, line);
                        }
                        _ => index += 1,
                    }
                }
            }
            _ => index += 1,
        }
    }
    index
}

// ---------------------------------------------------------------------------
// Action extraction
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum ActionProblemKind {
    UnknownKey { key: String },
    MissingLabel,
    NothingToRun,
    HandlerAndForm,
}

#[derive(Debug, Clone, PartialEq)]
struct ActionProblem {
    line: u64,
    kind: ActionProblemKind,
}

impl ActionProblem {
    fn message(&self) -> String {
        let allowed = ACTION_KEYS.join(", ");
        match &self.kind {
            ActionProblemKind::UnknownKey { key } => {
                let hint = if FIELD_ONLY_KEYS.contains(&key.as_str()) {
                    " Row actions are declarative: there is no `onClick`/`icon`/`title` support. Use `handler` for custom JavaScript and `label` for the button text."
                } else {
                    ""
                };
                format!(
                    "Row action uses unsupported property `{key}`, which `ResourceGrid` ignores, so the button never renders.{hint} \
                     Supported action properties: {allowed}. \
                     `label` is static button text; `labelExpression` is a JavaScript expression whose result must itself be a string (e.g. labelExpression: \"'Download PDF'\")."
                )
            }
            ActionProblemKind::MissingLabel => format!(
                "Row action has neither `label` nor `labelExpression`, so its button can never render. \
                 Use `label: 'Download PDF'` for a constant label, or `labelExpression: \"data.status === 'draft' ? 'Issue' : null\"` for a per-row label."
            ),
            ActionProblemKind::NothingToRun => format!(
                "Row action has nothing to run: provide `handler` (custom JavaScript), `form` (a dialog), or `actionExpression` (a path POSTed to the backend). Supported action properties: {allowed}."
            ),
            ActionProblemKind::HandlerAndForm => {
                "Row action sets both `handler` and `form`; only one of them may be used. Use `handler` for inline JavaScript, or `form` for a dialog that submits to an endpoint.".to_string()
            }
        }
    }
}

/// One property found directly inside an action object literal.
#[derive(Debug, Clone)]
struct Property {
    name: String,
    line: u64,
}

/// Finds the object literals that sit directly inside an `actions: [...]` array
/// and inspects their top-level properties.
fn scan_actions(source: &str) -> Vec<ActionProblem> {
    let tokens = tokenize(source);
    let mut problems = Vec::new();

    let mut index = 0;
    while index < tokens.len() {
        let is_actions_key = matches!(&tokens[index].tok, Tok::Ident(name) if name == "actions")
            && matches!(tokens.get(index + 1).map(|t| &t.tok), Some(Tok::Punct(':')))
            && matches!(tokens.get(index + 2).map(|t| &t.tok), Some(Tok::Punct('[')));
        if !is_actions_key {
            index += 1;
            continue;
        }

        // Walk the array body, collecting top-level object literals.
        let mut cursor = index + 3; // skip `actions`, `:` and the opening `[`
        let mut depth = 0usize; // nesting depth inside the array body
        let mut current: Option<Vec<Property>> = None;
        let mut current_line = 0u64;
        let mut object_depth = 0usize;

        while cursor < tokens.len() {
            let token = &tokens[cursor];
            match &token.tok {
                Tok::Punct(']') if depth == 0 => {
                    cursor += 1;
                    break;
                }
                Tok::Punct('[') | Tok::Punct('{') | Tok::Punct('(') => {
                    if depth == 0 && matches!(token.tok, Tok::Punct('{')) {
                        current = Some(Vec::new());
                        current_line = token.line;
                        object_depth = 1;
                        depth = 1;
                        cursor += 1;
                        continue;
                    }
                    depth += 1;
                    if object_depth > 0 {
                        object_depth += 1;
                    }
                }
                Tok::Punct(']') | Tok::Punct('}') | Tok::Punct(')') => {
                    depth = depth.saturating_sub(1);
                    if object_depth > 0 {
                        object_depth -= 1;
                        if object_depth == 0 {
                            if let Some(properties) = current.take() {
                                problems.extend(validate_object(&properties, current_line));
                            }
                        }
                    }
                }
                _ => {
                    if object_depth == 1 {
                        if let Some(properties) = current.as_mut() {
                            if let Tok::Ident(name) = &token.tok {
                                if matches!(
                                    tokens.get(cursor + 1).map(|t| &t.tok),
                                    Some(Tok::Punct(':'))
                                ) {
                                    properties.push(Property {
                                        name: name.clone(),
                                        line: token.line,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            cursor += 1;
        }

        index = cursor.max(index + 1);
    }

    problems
}

fn validate_object(properties: &[Property], object_line: u64) -> Vec<ActionProblem> {
    let mut problems = Vec::new();

    for property in properties {
        if !ACTION_KEYS.contains(&property.name.as_str()) {
            problems.push(ActionProblem {
                line: property.line,
                kind: ActionProblemKind::UnknownKey {
                    key: property.name.clone(),
                },
            });
        }
    }

    let has = |name: &str| properties.iter().any(|property| property.name == name);

    if !has("label") && !has("labelExpression") {
        problems.push(ActionProblem {
            line: object_line,
            kind: ActionProblemKind::MissingLabel,
        });
    }
    if has("handler") && has("form") {
        problems.push(ActionProblem {
            line: object_line,
            kind: ActionProblemKind::HandlerAndForm,
        });
    }
    if !has("handler") && !has("form") && !has("actionExpression") {
        problems.push(ActionProblem {
            line: object_line,
            kind: ActionProblemKind::NothingToRun,
        });
    }

    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    fn problems(source: &str) -> Vec<ActionProblem> {
        scan_actions(source)
    }

    #[test]
    fn accepts_static_label() {
        let source = r#"
export default defineResource({
  name: "sales",
  actions: [
    { label: "Download PDF", handler: "await api.openPdf('/pdf/invoice/' + data.id);" }
  ]
});
"#;
        assert!(problems(source).is_empty());
    }

    #[test]
    fn accepts_label_expression() {
        let source = r#"
  actions: [
    { labelExpression: "data.status === 'draft' ? 'Issue' : null", actionExpression: "/issue" }
  ]
"#;
        assert!(problems(source).is_empty());
    }

    #[test]
    fn accepts_form_action_and_ignores_nested_fields() {
        let source = r#"
  actions: [
    {
      label: "Adjust",
      form: {
        title: "Adjust Order",
        fields: [
          { name: "reason", type: "text", label: "Reason", required: true }
        ]
      }
    }
  ]
"#;
        assert!(problems(source).is_empty());
    }

    #[test]
    fn ignores_field_definitions_with_label() {
        let source = r#"
export default defineResource({
  fields: [
    field.text("invoiceNumber").label("Invoice #"),
    field.select("status").label("Status")
  ]
});
"#;
        assert!(problems(source).is_empty());
    }

    #[test]
    fn flags_unknown_property() {
        let source = r#"
  actions: [
    { onClick: (record) => open(record), title: "Invoice" }
  ]
"#;
        let found = problems(source);
        let keys: Vec<String> = found
            .iter()
            .filter_map(|problem| match &problem.kind {
                ActionProblemKind::UnknownKey { key } => Some(key.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(keys, vec!["onClick".to_string(), "title".to_string()]);
    }

    #[test]
    fn flags_missing_label() {
        let source = r#"
  actions: [
    { handler: "doThing();" }
  ]
"#;
        let found = problems(source);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, ActionProblemKind::MissingLabel);
    }

    #[test]
    fn flags_nothing_to_run() {
        let source = r#"
  actions: [
    { label: "Download PDF" }
  ]
"#;
        let found = problems(source);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, ActionProblemKind::NothingToRun);
    }

    #[test]
    fn flags_handler_and_form() {
        let source = r#"
  actions: [
    { label: "Both", handler: "x();", form: { title: "T" } }
  ]
"#;
        let found = problems(source);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, ActionProblemKind::HandlerAndForm);
    }

    #[test]
    fn handles_multiple_actions_and_following_code() {
        let source = r#"
  actions: [
    { labelExpression: "'Receive Payment'", form: { title: "Pay", fields: [] } },
    { label: "Download PDF", handler: "await api.openPdf('/pdf/invoice/' + data.id);" }
  ],
  titleExpression: "data.invoiceNumber"
});
"#;
        assert!(problems(source).is_empty());
    }

    #[test]
    fn commentary_and_strings_are_not_parsed() {
        let source = r#"
  // actions: [ { title: "commented out" } ]
  const note = "actions: [ { onClick: 1 } ]";
  actions: [
    { label: "PDF", handler: "const s = 'actions: [ {bad: 1} ]'; run(s);" }
  ]
"#;
        assert!(problems(source).is_empty());
    }

    #[test]
    fn skips_builder_chains() {
        // `action()` chains carry their config through builder methods, which
        // this static scan cannot see; they are not object literals.
        let source = r#"
  actions: [action().permission("a.b").labelExpression("'Go'").handler("go();")]
"#;
        assert!(problems(source).is_empty());
    }
}
