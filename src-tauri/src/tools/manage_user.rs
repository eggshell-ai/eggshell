use async_trait::async_trait;
use serde_json::{json, Map, Value};
use std::path::Path;

use crate::llm::{LlmResult, Tool};

/// Manages the sign-in accounts for the active project.
///
/// The authoritative record lives in the project's Symfony backend; this tool
/// drives `php bin/console app:user:manage` there and mirrors the result into
/// the desktop's local `users` table so the chat sidebar can list the accounts.
pub struct ManageUserTool {
    parameters: Map<String, Value>,
}

impl ManageUserTool {
    pub fn new() -> Self {
        Self {
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["create", "drop", "add", "list"],
                        "description": "create = new user with a random password, drop = delete a user, add = assign a role to an existing user, list = show users."
                    },
                    "email": {
                        "type": "string",
                        "description": "User email address. Required for create, drop and add."
                    },
                    "role": {
                        "type": "string",
                        "description": "Role name (e.g. ROLE_MANAGER). Used by create and add. Defaults to ROLE_USER."
                    },
                    "password": {
                        "type": "string",
                        "description": "Optional password. When omitted on create, a random one is generated and returned."
                    },
                    "projectPath": {
                        "type": "string",
                        "description": "Overrides the target project directory."
                    }
                },
                "required": ["action"]
            })
            .as_object()
            .expect("manage_user parameters must be an object")
            .clone(),
        }
    }
}

impl Default for ManageUserTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ManageUserTool {
    fn name(&self) -> &str {
        "manage_user"
    }

    fn description(&self) -> &str {
        "Creates, drops, or modifies the project's sign-in users (and lists them). New users get a random password which is shown in the chat login list."
    }

    fn parameters(&self) -> &Map<String, Value> {
        &self.parameters
    }

    async fn execute(&self, args: Value) -> LlmResult<Value> {
        let action = args
            .get("action")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|action| !action.is_empty())
            .ok_or_else(|| "action is required".to_string())?
            .to_lowercase();

        if !matches!(action.as_str(), "create" | "drop" | "add" | "list") {
            return Err(format!("unsupported action '{action}'").into());
        }

        let email = args
            .get("email")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("")
            .to_string();
        let role = args
            .get("role")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|role| !role.is_empty())
            .unwrap_or("ROLE_USER")
            .to_string();
        let mut password = args
            .get("password")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        if action != "list" && email.is_empty() {
            return Err(format!("email is required for the '{action}' action").into());
        }

        let project_path = args
            .get("projectPath")
            .and_then(Value::as_str)
            .filter(|path| !path.trim().is_empty())
            .ok_or_else(|| "projectPath is required".to_string())?
            .to_string();
        let backend_dir = Path::new(&project_path).join("backend");
        if !backend_dir.is_dir() {
            return Err(format!("backend directory not found at {}", backend_dir.display()).into());
        }

        // A password is only generated here when the model did not supply one;
        // the console command generates its own fallback otherwise, but the
        // generated value must be echoed back so it can be shown in the UI.
        let generated = action == "create" && password.is_empty();
        if generated {
            password = generate_password(12);
        }

        let mut console_args: Vec<String> = vec![
            "bin/console".to_string(),
            "app:user:manage".to_string(),
            action.clone(),
        ];
        if !email.is_empty() {
            console_args.push(email.clone());
        } else {
            // Keep positional slots aligned when there is no email (list).
            console_args.push(String::new());
        }
        console_args.push(password.clone());
        console_args.push(role.clone());
        console_args.push("--json".to_string());

        let stdout = crate::llm::symfony::run_php_capture(&console_args, &backend_dir).await?;
        let parsed: Value = parse_json_output(&stdout);
        if parsed.get("success").and_then(Value::as_bool) == Some(false) {
            let message = parsed
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("user management command failed");
            return Err(message.to_string().into());
        }

        // Mirror the change into the local users table when we have a project id.
        if let Some(pool) = crate::db::pool() {
            if let Ok(Some(project_id)) =
                crate::db::UsersRepository::project_id_for_path(pool, &project_path).await
            {
                match action.as_str() {
                    "create" => {
                        let _ = crate::db::UsersRepository::upsert(
                            pool,
                            &crate::db::User {
                                id: 0,
                                project_id,
                                email: email.clone(),
                                role: normalize_role(&role),
                                password: password.clone(),
                            },
                        )
                        .await;
                    }
                    "drop" => {
                        let _ = crate::db::UsersRepository::delete(pool, project_id, &email).await;
                    }
                    "add" => {
                        let _ = crate::db::UsersRepository::upsert(
                            pool,
                            &crate::db::User {
                                id: 0,
                                project_id,
                                email: email.clone(),
                                role: normalize_role(&role),
                                // Preserve the existing password; upsert keeps it only
                                // when we echo the previous value, so read it back.
                                password: current_password(pool, project_id, &email)
                                    .await
                                    .unwrap_or_default(),
                            },
                        )
                        .await;
                    }
                    _ => {}
                }
            }
        }

        let message = match action.as_str() {
            "create" => format!("User '{email}' created with role '{}'.", normalize_role(&role)),
            "drop" => format!("User '{email}' dropped."),
            "add" => format!("Role '{}' added to user '{email}'.", normalize_role(&role)),
            _ => "Users listed.".to_string(),
        };

        Ok(json!({
            "success": true,
            "message": message,
            "details": {
                "action": action,
                "email": email,
                "role": normalize_role(&role),
                "password": password,
                "generated": generated,
                "result": parsed
            }
        }))
    }
}

async fn current_password(
    pool: &sqlx::SqlitePool,
    project_id: i64,
    email: &str,
) -> Option<String> {
    let users = crate::db::UsersRepository::list(pool, project_id).await.ok()?;
    users
        .into_iter()
        .find(|user| user.email.eq_ignore_ascii_case(email))
        .map(|user| user.password)
}

fn normalize_role(role: &str) -> String {
    let upper = role.trim().to_uppercase();
    if upper.starts_with("ROLE_") {
        upper
    } else {
        format!("ROLE_{upper}")
    }
}

fn generate_password(length: usize) -> String {
    // 12-character alphanumeric password built from UUIDs, avoiding ambiguous
    // characters. Good enough for a throwaway local development account.
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnpqrstuvwxyz23456789";
    let mut out = String::with_capacity(length);
    while out.len() < length {
        let uuid = uuid::Uuid::new_v4();
        for byte in uuid.as_bytes() {
            if out.len() >= length {
                break;
            }
            out.push(ALPHABET[(*byte as usize) % ALPHABET.len()] as char);
        }
    }
    out
}

fn parse_json_output(stdout: &str) -> Value {
    // The console may emit warnings before the JSON payload; take the first
    // line that parses as a JSON object.
    for line in stdout.lines().rev() {
        let trimmed = line.trim();
        if trimmed.starts_with('{') && trimmed.ends_with('}') {
            if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
                return value;
            }
        }
    }
    json!({ "raw": stdout.trim() })
}
