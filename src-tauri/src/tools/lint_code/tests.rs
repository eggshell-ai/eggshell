//! End-to-end tests for the tool itself (argument handling + aggregation).

use super::*;
use serde_json::json;
use std::fs;
use std::path::PathBuf;

fn templates_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("templates")
}

#[test]
fn test_tool_name_and_params() {
    let tool = LintCodeTool::new();
    assert_eq!(tool.name(), "lint_code");
    assert!(tool.parameters().contains_key("properties"));

    let properties = tool.parameters()["properties"].as_object().unwrap();
    assert!(properties.contains_key("checks"));
    assert!(properties.contains_key("skipChecks"));
    assert!(properties.contains_key("deep"));
}

#[tokio::test]
async fn test_backend_lint_on_clean_template() {
    let tool = LintCodeTool::new();
    let args = json!({
        "shell": "backend",
        "projectPath": templates_dir().to_str().unwrap()
    });

    let result = tool.execute(args).await.expect("Tool execution failed");
    assert_eq!(result["success"], true);
    assert_eq!(result["totalErrors"], 0);
    assert_eq!(result["totalWarnings"], 0);
}

#[tokio::test]
async fn test_backend_lint_detects_syntax_error() {
    let tool = LintCodeTool::new();
    let temp_dir = std::env::temp_dir().join("eggshell_lint_test");
    let backend_src = temp_dir.join("backend").join("src");
    fs::create_dir_all(&backend_src).unwrap();

    let bad_file = backend_src.join("BadSyntax.php");
    fs::write(&bad_file, "<?php\n\nfunction broken() {\n    echo 1 +\n}\n").unwrap();

    let args = json!({
        "shell": "backend",
        "projectPath": temp_dir.to_str().unwrap()
    });

    let result = tool.execute(args).await.expect("Tool execution failed");
    assert_eq!(result["success"], false);
    assert!(result["totalErrors"].as_u64().unwrap() >= 1);

    let issues = result["issues"].as_array().unwrap();
    let bad_issue = issues
        .iter()
        .find(|issue| issue["file"].as_str().unwrap().contains("BadSyntax.php"));
    assert!(bad_issue.is_some());
    assert_eq!(bad_issue.unwrap()["line"], 5);

    let _ = fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_frontend_lint_detects_invalid_menu_json() {
    let tool = LintCodeTool::new();
    let temp_dir = std::env::temp_dir().join("eggshell_lint_menu_json_test");
    let frontend_menu_dir = temp_dir.join("frontend").join("src").join("menu-items");
    fs::create_dir_all(&frontend_menu_dir).unwrap();

    let menu_file = frontend_menu_dir.join("menu.json");
    fs::write(
        &menu_file,
        "[\n  {\n    \"name\": \"Test\",\n    \"route\": \"/test\"\n  ,\n]",
    )
    .unwrap();

    let args = json!({
        "shell": "frontend",
        "projectPath": temp_dir.to_str().unwrap()
    });

    let result = tool.execute(args).await.expect("Tool execution failed");
    assert_eq!(result["success"], false);
    assert!(result["totalErrors"].as_u64().unwrap() >= 1);

    let issues = result["issues"].as_array().unwrap();
    let menu_issue = issues
        .iter()
        .find(|issue| issue["file"].as_str().unwrap().contains("src/menu-items/menu.json"));
    assert!(menu_issue.is_some());
    let issue = menu_issue.unwrap();
    assert_eq!(issue["severity"], "error");
    assert_eq!(issue["source"], "menu-json");

    let _ = fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_removed_route_annotation_is_caught_without_php() {
    let tool = LintCodeTool::new();
    let temp_dir = std::env::temp_dir().join("eggshell_lint_removed_route_import");
    let controllers = temp_dir.join("backend").join("src").join("Controller");
    fs::create_dir_all(&controllers).unwrap();

    // Note: no `bin/console`, no `vendor/` — the static check must still fire.
    fs::write(
        controllers.join("SalesController.php"),
        "<?php\n\nnamespace App\\Controller;\n\nuse App\\Resource\\ResourceController;\nuse Symfony\\Component\\Routing\\Annotation\\Route;\n\n#[Route('/api/sales', name: 'sales.')]\nfinal class SalesController extends ResourceController\n{\n}\n",
    )
    .unwrap();

    let args = json!({
        "shell": "backend",
        "projectPath": temp_dir.to_str().unwrap()
    });

    let result = tool.execute(args).await.expect("Tool execution failed");
    assert_eq!(result["success"], false);

    let issues = result["issues"].as_array().unwrap();
    let issue = issues
        .iter()
        .find(|issue| issue["source"] == "symfony-route-import")
        .expect("removed import must be reported");
    assert_eq!(issue["line"], 6);
    assert_eq!(issue["file"], "src/Controller/SalesController.php");
    assert!(issue["message"]
        .as_str()
        .unwrap()
        .contains("Routing\\Annotation\\Route"));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_check_selection_and_skip() {
    let tool = LintCodeTool::new();
    let temp_dir = std::env::temp_dir().join("eggshell_lint_selection");
    let controllers = temp_dir.join("backend").join("src").join("Controller");
    fs::create_dir_all(&controllers).unwrap();
    fs::write(
        controllers.join("Bad.php"),
        "<?php\nuse Symfony\\Component\\Routing\\Annotation\\Route;\n",
    )
    .unwrap();

    // Only the php-syntax check runs: the import check is filtered out.
    let args = json!({
        "shell": "backend",
        "deep": false,
        "checks": ["php-syntax"],
        "projectPath": temp_dir.to_str().unwrap()
    });
    let result = tool.execute(args).await.unwrap();
    assert_eq!(result["success"], true);

    // Skipping it while selecting everything keeps it out too.
    let args = json!({
        "shell": "backend",
        "deep": false,
        "skipChecks": ["symfony-route-import"],
        "projectPath": temp_dir.to_str().unwrap()
    });
    let result = tool.execute(args).await.unwrap();
    assert_eq!(result["success"], true);

    // Without the skip, it is reported.
    let args = json!({
        "shell": "backend",
        "deep": false,
        "projectPath": temp_dir.to_str().unwrap()
    });
    let result = tool.execute(args).await.unwrap();
    assert_eq!(result["success"], false);

    let _ = fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_unknown_check_id_is_rejected() {
    let tool = LintCodeTool::new();
    let args = json!({
        "shell": "backend",
        "checks": ["does-not-exist"]
    });

    let error = tool.execute(args).await.expect_err("must reject unknown id");
    assert!(error.to_string().contains("Unknown check id"));
}

#[tokio::test]
async fn test_deep_checks_are_skipped_when_disabled() {
    let tool = LintCodeTool::new();
    let args = json!({
        "shell": "backend",
        "deep": false,
        "projectPath": templates_dir().to_str().unwrap()
    });

    let result = tool.execute(args).await.unwrap();
    assert_eq!(result["success"], true);
    let message = result["message"].as_str().unwrap();
    assert!(message.contains("Skipped (requires `deep: true`)"));
    assert!(message.contains("symfony-routes"));
}

#[tokio::test]
async fn test_rejects_unsafe_path() {
    let tool = LintCodeTool::new();
    let args = json!({
        "shell": "backend",
        "path": "../secrets"
    });

    let error = tool.execute(args).await.expect_err("must reject traversal");
    assert!(error.to_string().contains("safe relative path"));
}

/// Smoke test against a real generated project. Skipped unless
/// `EGGSHELL_LINT_PROJECT` points at a project directory containing a `backend/`
/// with installed dependencies, e.g.
/// `$env:EGGSHELL_LINT_PROJECT='E:\Faheem\Projects\sales-system'; cargo test --lib manual_backend_route_audit`.
#[tokio::test]
async fn manual_backend_route_audit() {
    let Some(project) = std::env::var_os("EGGSHELL_LINT_PROJECT") else {
        return;
    };
    let project = PathBuf::from(project);
    if !project.join("backend").is_dir() {
        return;
    }

    let tool = LintCodeTool::new();
    let result = tool
        .execute(json!({
            "shell": "backend",
            "deep": true,
            "projectPath": project.to_str().unwrap()
        }))
        .await
        .expect("Tool execution failed");

    println!("{}", serde_json::to_string_pretty(&result).unwrap());

    let route_issues: Vec<_> = result["issues"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|issue| {
            matches!(
                issue["source"].as_str(),
                Some("symfony-routes" | "symfony-route-import")
            )
        })
        .collect();

    println!("route issues: {route_issues:#?}");
}
