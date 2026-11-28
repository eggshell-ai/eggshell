//! Checks that catch controllers which exist but never reach the router.
//!
//! The motivating failure: a controller importing the Symfony **6+ removed**
//! `Symfony\Component\Routing\Annotation\Route` compiles fine and even passes
//! `php -l`, but the attribute loader silently skips every `#[Route]` because
//! the class does not exist. Nothing is thrown, `debug:router` still exits `0`,
//! and the only visible symptom is a 404 on the URL the controller was supposed
//! to serve.
//!
//! Two checks cover it:
//!
//! - [`DeprecatedRouteImportCheck`] is a pure static scan for the removed
//!   namespace. It is free and works before `composer install`.
//! - [`RouteRegistrationCheck`] compares declared controller prefixes against
//!   the real routing table (`debug:router --format=json`) to find controllers
//!   that contribute no routes, routes that lost their prefix, and controllers
//!   whose routes were shadowed by unprefixed names.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Stdio;

use serde_json::Value;

use super::check::{Check, CheckContext, Shell, ShellScope, Tier};
use super::issue::Issue;
use super::php::{collect_php_files, console_ready, hide_window};
use crate::llm::LlmResult;

/// The namespace Symfony removed in 6.0. Using it makes `#[Route]` a no-op.
const REMOVED_ROUTE_NAMESPACE: &str = "Symfony\\Component\\Routing\\Annotation";
/// The namespace that must be used instead.
const CURRENT_ROUTE_NAMESPACE: &str = "Symfony\\Component\\Routing\\Attribute";

/// Route names inherited from `ResourceController`, in registration order.
const RESOURCE_METHODS: [&str; 5] = ["index", "show", "store", "update", "destroy"];

// -----------------------------------------------------------------------------
// Check A: removed namespace import
// -----------------------------------------------------------------------------

pub(crate) struct DeprecatedRouteImportCheck;

impl Check for DeprecatedRouteImportCheck {
    fn id(&self) -> &'static str {
        "symfony-route-import"
    }

    fn description(&self) -> &'static str {
        "Rejects the removed `Routing\\Annotation\\Route` import (silently disables routes)."
    }

    fn shells(&self) -> ShellScope {
        ShellScope::Backend
    }

    fn tier(&self) -> Tier {
        Tier::Static
    }

    fn run(&self, ctx: &CheckContext) -> LlmResult<Vec<Issue>> {
        let backend_dir = ctx.shell_dir(Shell::Backend);
        let controller_dir = backend_dir.join("src").join("Controller");
        if !controller_dir.is_dir() {
            return Ok(Vec::new());
        }

        let mut files = Vec::new();
        collect_php_files(&controller_dir, &mut files);

        let mut issues = Vec::new();
        for file in files {
            let Ok(contents) = fs::read_to_string(&file) else {
                continue;
            };
            let display = display_path(&backend_dir, &file);

            for (index, line) in contents.lines().enumerate() {
                if !line.contains(REMOVED_ROUTE_NAMESPACE) {
                    continue;
                }

                issues.push(Issue::error(
                    Shell::Backend,
                    self.id(),
                    display.clone(),
                    index as u64 + 1,
                    format!(
                        "`{REMOVED_ROUTE_NAMESPACE}\\Route` was removed in Symfony 6 and does not \
                         exist in this project. PHP never loads the class, so every `#[Route]` in \
                         this controller is silently ignored and its endpoints return 404 without \
                         any error. Replace the import with \
                         `use {CURRENT_ROUTE_NAMESPACE}\\Route;`."
                    ),
                ));
            }
        }

        Ok(issues)
    }
}

// -----------------------------------------------------------------------------
// Check B: declared prefix vs registered routes
// -----------------------------------------------------------------------------

pub(crate) struct RouteRegistrationCheck;

impl Check for RouteRegistrationCheck {
    fn id(&self) -> &'static str {
        "symfony-routes"
    }

    fn description(&self) -> &'static str {
        "Audits controller route prefixes against `debug:router` (catches unregistered controllers)."
    }

    fn shells(&self) -> ShellScope {
        ShellScope::Backend
    }

    fn tier(&self) -> Tier {
        Tier::Command
    }

    fn requires_deep(&self) -> bool {
        true
    }

    fn run(&self, ctx: &CheckContext) -> LlmResult<Vec<Issue>> {
        let backend_dir = ctx.shell_dir(Shell::Backend);
        let controller_dir = backend_dir.join("src").join("Controller");
        if !controller_dir.is_dir() || !console_ready(&backend_dir) {
            return Ok(Vec::new());
        }

        let declarations = parse_controller_declarations(&backend_dir);
        if declarations.is_empty() {
            return Ok(Vec::new());
        }

        let routes = match read_router_table(&backend_dir) {
            Ok(routes) => routes,
            Err(issue) => return Ok(vec![issue]),
        };

        Ok(audit_registrations(&declarations, &routes))
    }
}

/// Runs `debug:router --format=json` and flattens it into `controller -> routes`.
fn read_router_table(backend_dir: &Path) -> Result<Vec<RegisteredRoute>, Issue> {
    let mut cmd = crate::llm::symfony::new_php_command();
    cmd.args(["bin/console", "debug:router", "--format=json", "--no-interaction"])
        .current_dir(backend_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    hide_window(&mut cmd);

    let output = cmd.output().map_err(|error| {
        Issue::error(
            Shell::Backend,
            "symfony-routes",
            "bin/console",
            1,
            format!("Could not run `debug:router`: {error}"),
        )
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        let detail = if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        };
        return Err(Issue::error(
            Shell::Backend,
            "symfony-routes",
            "bin/console",
            1,
            format!(
                "`debug:router` failed, so routing cannot be verified: {}",
                truncate(detail, 1200)
            ),
        ));
    }

    // Warnings may precede the JSON document; start from the first '{'.
    let json_slice = stdout
        .find('{')
        .map(|start| &stdout[start..])
        .unwrap_or(stdout.as_ref());

    let parsed: Value = serde_json::from_str(json_slice).map_err(|error| {
        Issue::error(
            Shell::Backend,
            "symfony-routes",
            "bin/console",
            1,
            format!("Could not parse `debug:router --format=json` output: {error}"),
        )
    })?;

    Ok(parse_router_table(&parsed))
}

/// Flattens the router JSON into a list of routes we can reason about.
pub(crate) fn parse_router_table(value: &Value) -> Vec<RegisteredRoute> {
    let Some(entries) = value.as_object() else {
        return Vec::new();
    };

    let mut routes = Vec::new();
    for (name, route) in entries {
        let path = route
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let controller = route
            .get("defaults")
            .and_then(|defaults| defaults.get("_controller"))
            .and_then(Value::as_str)
            .unwrap_or_default();

        // `_controller` looks like `App\Controller\XController::index` and may
        // carry a trailing `()` for invokable controllers.
        let controller = controller.trim_end_matches("()");
        let (class, method) = controller
            .split_once("::")
            .map(|(class, method)| (class, method))
            .unwrap_or((controller, ""));

        routes.push(RegisteredRoute {
            name: name.clone(),
            path,
            class: class.to_string(),
            method: method.to_string(),
        });
    }

    routes
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RegisteredRoute {
    pub name: String,
    pub path: String,
    pub class: String,
    pub method: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RouteAttribute {
    pub path: String,
    pub name: String,
    /// 1-based line of the attribute.
    pub line: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ControllerDeclaration {
    pub fqcn: String,
    pub file: String,
    /// Class-level `#[Route]`, if the controller declares one.
    pub route: Option<RouteAttribute>,
    pub extends_resource_controller: bool,
}

/// Scans `backend/src/Controller/**/*.php` for namespace, class and the
/// class-level `#[Route]` attribute.
pub(crate) fn parse_controller_declarations(
    backend_dir: &Path,
) -> Vec<ControllerDeclaration> {
    let controller_dir = backend_dir.join("src").join("Controller");
    let mut files = Vec::new();
    collect_php_files(&controller_dir, &mut files);
    files.sort();

    let mut declarations = Vec::new();
    for file in files {
        let Ok(contents) = fs::read_to_string(&file) else {
            continue;
        };
        if let Some(declaration) = parse_controller_source(&contents, display_path(backend_dir, &file))
        {
            declarations.push(declaration);
        }
    }

    declarations
}

/// Parses a single PHP source file. Returns `None` when it is not a controller
/// (for example an abstract helper without a class declaration).
pub(crate) fn parse_controller_source(
    source: &str,
    file: String,
) -> Option<ControllerDeclaration> {
    let mut namespace = String::new();
    let mut class_name: Option<String> = None;
    let mut extends_resource_controller = false;
    let mut pending_route: Option<RouteAttribute> = None;

    for (index, raw_line) in source.lines().enumerate() {
        let line = raw_line.trim();
        let line_number = index as u64 + 1;

        if namespace.is_empty() {
            if let Some(rest) = line.strip_prefix("namespace ") {
                namespace = rest.trim_end_matches(';').trim().to_string();
                continue;
            }
        }

        if class_name.is_none() {
            if let Some(attribute) = parse_route_attribute(line, line_number) {
                pending_route = Some(attribute);
                continue;
            }

            if let Some((name, extends)) = parse_class_declaration(line) {
                class_name = Some(name);
                extends_resource_controller = extends;
            }
            continue;
        }
    }

    let class_name = class_name?;
    // The attribute only belongs to the class if it appeared before it.
    let fqcn = if namespace.is_empty() {
        class_name
    } else {
        format!("{namespace}\\{class_name}")
    };

    Some(ControllerDeclaration {
        fqcn,
        file,
        route: pending_route,
        extends_resource_controller,
    })
}

/// Recognises `#[Route('/api/x', name: 'x.')]` on a single line.
fn parse_route_attribute(line: &str, line_number: u64) -> Option<RouteAttribute> {
    let body = line.strip_prefix("#[Route(")?;
    let body = body.strip_suffix(")]")?;

    let path = first_quoted(body)?;
    let name = named_argument(body, "name").unwrap_or_default();

    Some(RouteAttribute {
        path,
        name,
        line: line_number,
    })
}

/// Recognises `class XController extends ResourceController` (optionally `final`).
fn parse_class_declaration(line: &str) -> Option<(String, bool)> {
    let rest = ["final class ", "abstract class ", "class "]
        .iter()
        .find_map(|prefix| line.strip_prefix(prefix))?;

    let name = rest
        .split(|c: char| c.is_whitespace() || c == '{' || c == ':')
        .next()?
        .trim();

    if !is_identifier(name) {
        return None;
    }

    let extends_resource_controller = line.contains("extends ResourceController")
        || line.contains("extends \\App\\Resource\\ResourceController");

    Some((name.to_string(), extends_resource_controller))
}

fn is_identifier(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .next()
            .map(|c| c.is_ascii_alphabetic() || c == '_')
            .unwrap_or(false)
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Returns the first single-quoted or double-quoted literal in `body`.
fn first_quoted(body: &str) -> Option<String> {
    let mut chars = body.char_indices();
    while let Some((start, ch)) = chars.next() {
        if ch != '\'' && ch != '"' {
            continue;
        }
        let quote = ch;
        for (end, inner) in chars.by_ref() {
            if inner == quote {
                return Some(body[start + 1..end].to_string());
            }
        }
        return None;
    }
    None
}

/// Reads a `name: 'value'` named argument from an attribute argument list.
fn named_argument(body: &str, argument: &str) -> Option<String> {
    for segment in body.split(',') {
        let Some((key, value)) = segment.split_once(':') else {
            continue;
        };
        if key.trim() == argument {
            return first_quoted(value);
        }
    }
    None
}

/// Compares declarations against the routing table.
///
/// Split from the command runner so it can be unit-tested with a captured
/// `debug:router` payload and no PHP process.
pub(crate) fn audit_registrations(
    declarations: &[ControllerDeclaration],
    routes: &[RegisteredRoute],
) -> Vec<Issue> {
    let mut by_class: BTreeMap<&str, Vec<&RegisteredRoute>> = BTreeMap::new();
    for route in routes {
        by_class.entry(route.class.as_str()).or_default().push(route);
    }

    let unprefixed_resource_routes: Vec<&str> = routes
        .iter()
        .filter(|route| route.class.starts_with("App\\Controller"))
        .filter(|route| RESOURCE_METHODS.contains(&route.method.as_str()))
        .filter(|route| !route.path.starts_with("/api"))
        .map(|route| route.name.as_str())
        .collect();

    let mut issues = Vec::new();

    for declaration in declarations {
        let registered = by_class
            .get(declaration.fqcn.as_str())
            .cloned()
            .unwrap_or_default();

        match (&declaration.route, registered.is_empty()) {
            // The controller was never turned into routes at all. This is the
            // signature of an ignored class-level attribute.
            (Some(attribute), true) => {
                let mut message = format!(
                    "Controller `{}` declares `#[Route('{}', name: '{}')]` but contributes no \
                     routes to the router. Its endpoints will 404. This happens when the \
                     `#[Route]` attribute is not recognised — usually because the file imports \
                     `{REMOVED_ROUTE_NAMESPACE}\\Route` instead of `{CURRENT_ROUTE_NAMESPACE}\\Route`.",
                    declaration.fqcn, attribute.path, attribute.name
                );

                if !unprefixed_resource_routes.is_empty() {
                    message.push_str(&format!(
                        " Its inherited resource routes were also shadowed by unprefixed route \
                         names ({}), which belong to another controller.",
                        unprefixed_resource_routes.join(", ")
                    ));
                }

                issues.push(Issue::error(
                    Shell::Backend,
                    "symfony-routes",
                    declaration.file.clone(),
                    attribute.line,
                    message,
                ));
            }

            // Routes exist, but none is under the prefix the controller asked for.
            (Some(attribute), false) => {
                let prefix = attribute.path.trim_end_matches('/');
                let matches_prefix = registered
                    .iter()
                    .any(|route| route.path.starts_with(prefix));

                if !matches_prefix {
                    issues.push(Issue::error(
                        Shell::Backend,
                        "symfony-routes",
                        declaration.file.clone(),
                        attribute.line,
                        format!(
                            "Controller `{}` declares the prefix `{}` but its routes registered \
                             at {:?}. The class-level `#[Route]` prefix was not applied, so the \
                             API is exposed at the wrong URLs (expected paths starting with `{}`).",
                            declaration.fqcn,
                            attribute.path,
                            registered
                                .iter()
                                .map(|route| route.path.as_str())
                                .collect::<Vec<_>>(),
                            attribute.path
                        ),
                    ));
                }
            }

            // A resource controller with no prefix at all.
            (None, _) if declaration.extends_resource_controller => {
                issues.push(Issue::warning(
                    Shell::Backend,
                    "symfony-routes",
                    declaration.file.clone(),
                    1,
                    format!(
                        "Controller `{}` extends `ResourceController` but has no class-level \
                         `#[Route]` prefix. Its inherited endpoints will be registered at the \
                         application root (`/`, `/{{id}}`) instead of under `/api/...`.",
                        declaration.fqcn
                    ),
                ));
            }

            _ => {}
        }
    }

    issues
}

fn display_path(backend_dir: &Path, file: &Path) -> String {
    file.strip_prefix(backend_dir)
        .unwrap_or(file)
        .to_string_lossy()
        .replace('\\', "/")
}

fn truncate(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.to_string();
    }
    let truncated: String = value.chars().take(limit).collect();
    format!("{truncated}…")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn declaration(
        fqcn: &str,
        file: &str,
        route: Option<(&str, &str, u64)>,
        resource: bool,
    ) -> ControllerDeclaration {
        ControllerDeclaration {
            fqcn: fqcn.to_string(),
            file: file.to_string(),
            route: route.map(|(path, name, line)| RouteAttribute {
                path: path.to_string(),
                name: name.to_string(),
                line,
            }),
            extends_resource_controller: resource,
        }
    }

    #[test]
    fn parses_route_attribute_and_namespace() {
        let source = r#"<?php

namespace App\Controller;

use App\Resource\ResourceController;
use Symfony\Component\Routing\Annotation\Route;

#[Route('/api/sales', name: 'sales.')]
final class SalesController extends ResourceController
{
    protected function getEntityClass(): string
    {
        return Sale::class;
    }
}
"#;

        let parsed = parse_controller_source(source, "src/Controller/SalesController.php".into())
            .expect("controller should parse");

        assert_eq!(parsed.fqcn, "App\\Controller\\SalesController");
        assert!(parsed.extends_resource_controller);
        let route = parsed.route.expect("class route");
        assert_eq!(route.path, "/api/sales");
        assert_eq!(route.name, "sales.");
        assert_eq!(route.line, 8);
    }

    #[test]
    fn parses_nested_namespace_without_route() {
        let source = r#"<?php

namespace App\Controller\Auth;

class PermissionController
{
}
"#;
        let parsed =
            parse_controller_source(source, "src/Controller/Auth/PermissionController.php".into())
                .expect("controller should parse");
        assert_eq!(parsed.fqcn, "App\\Controller\\Auth\\PermissionController");
        assert!(parsed.route.is_none());
        assert!(!parsed.extends_resource_controller);
    }

    #[test]
    fn ignores_non_controller_files() {
        assert!(parse_controller_source("<?php\n\ninterface Foo {}\n", "Foo.php".into()).is_none());
    }

    #[test]
    fn detects_removed_route_namespace() {
        let root = std::env::temp_dir().join("eggshell_lint_route_import");
        let _ = fs::remove_dir_all(&root);
        let controllers = root.join("backend").join("src").join("Controller");
        fs::create_dir_all(&controllers).unwrap();

        fs::write(
            controllers.join("SalesController.php"),
            "<?php\n\nnamespace App\\Controller;\n\nuse Symfony\\Component\\Routing\\Annotation\\Route;\n\n#[Route('/api/sales', name: 'sales.')]\nfinal class SalesController\n{\n}\n",
        )
        .unwrap();
        fs::write(
            controllers.join("OkController.php"),
            "<?php\n\nuse Symfony\\Component\\Routing\\Attribute\\Route;\n",
        )
        .unwrap();

        let ctx = CheckContext {
            project_path: &root,
            relative_path: None,
            deep: false,
        };
        let issues = DeprecatedRouteImportCheck.run(&ctx).unwrap();

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].source, "symfony-route-import");
        assert_eq!(issues[0].file, "src/Controller/SalesController.php");
        assert_eq!(issues[0].line, 5);
        assert!(issues[0].message.contains("Attribute\\Route"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn parses_router_table_into_routes() {
        let payload = json!({
            "index": {
                "path": "/",
                "defaults": { "_controller": "App\\Controller\\SalesController::index" }
            },
            "app_api_login": {
                "path": "/api/login",
                "defaults": { "_controller": "App\\Controller\\Auth\\ApiLoginController::index" }
            }
        });

        let routes = parse_router_table(&payload);
        assert_eq!(routes.len(), 2);

        let sales = routes.iter().find(|r| r.name == "index").unwrap();
        assert_eq!(sales.path, "/");
        assert_eq!(sales.class, "App\\Controller\\SalesController");
        assert_eq!(sales.method, "index");
    }

    /// Replays the real broken routing table captured from a generated project
    /// where `SalesController` and `PaymentController` imported the removed
    /// annotation namespace.
    #[test]
    fn audit_flags_unregistered_controllers() {
        let declarations = vec![
            declaration(
                "App\\Controller\\CustomerController",
                "src/Controller/CustomerController.php",
                Some(("/api/customers", "customers.", 7)),
                true,
            ),
            declaration(
                "App\\Controller\\SalesController",
                "src/Controller/SalesController.php",
                Some(("/api/sales", "sales.", 14)),
                true,
            ),
            declaration(
                "App\\Controller\\PaymentController",
                "src/Controller/PaymentController.php",
                Some(("/api/payments", "payments.", 9)),
                true,
            ),
        ];

        let payload = json!({
            "customers.index": {
                "path": "/api/customers",
                "defaults": { "_controller": "App\\Controller\\CustomerController::index" }
            },
            "customers.show": {
                "path": "/api/customers/{id}",
                "defaults": { "_controller": "App\\Controller\\CustomerController::show" }
            },
            "index": {
                "path": "/",
                "defaults": { "_controller": "App\\Controller\\SalesController::index" }
            },
            "show": {
                "path": "/{id}",
                "defaults": { "_controller": "App\\Controller\\SalesController::show" }
            }
        });

        let routes = parse_router_table(&payload);
        let issues = audit_registrations(&declarations, &routes);

        // CustomerController is healthy.
        assert!(!issues.iter().any(|i| i.file.contains("CustomerController")));

        // SalesController registered routes, but not under its declared prefix.
        let sales = issues
            .iter()
            .find(|i| i.file.contains("SalesController"))
            .expect("SalesController must be flagged");
        assert!(sales.message.contains("prefix"));
        assert_eq!(sales.line, 14);

        // PaymentController registered nothing at all.
        let payment = issues
            .iter()
            .find(|i| i.file.contains("PaymentController"))
            .expect("PaymentController must be flagged");
        assert!(payment.message.contains("contributes no routes"));
        assert!(payment.message.contains("Annotation\\Route"));

        assert_eq!(issues.len(), 2);
        assert!(issues.iter().all(|i| i.is_error()));
    }

    #[test]
    fn audit_warns_when_resource_controller_has_no_prefix() {
        let declarations = vec![declaration(
            "App\\Controller\\OrphanController",
            "src/Controller/OrphanController.php",
            None,
            true,
        )];
        let issues = audit_registrations(&declarations, &[]);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].severity.as_str(), "warning");
    }
}
