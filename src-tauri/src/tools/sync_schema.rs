use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

use crate::llm::{LlmResult, Tool};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Field {
    name: String,
    #[serde(rename = "type")]
    field_type: String,
    label: Option<String>,
    table: Option<bool>,
    form: Option<bool>,
    detail: Option<bool>,
    length: Option<u64>,
    required: Option<bool>,
    unique: Option<bool>,
    email: Option<bool>,
    phone: Option<bool>,
    source: Option<String>,
    sortable: Option<bool>,
    searchable: Option<bool>,
    filterable: Option<bool>,
    password: Option<bool>,
    accept: Option<String>,
    max_size: Option<u64>,
    min_size: Option<u64>,
    resource: Option<Value>,
    map: Option<String>,
    columns: Option<Value>,
    default: Option<Value>,
    true_label: Option<String>,
    false_label: Option<String>,
    options: Option<Map<String, Value>>,
    messages: Option<Value>,
    visible_when: Option<String>,
    scale: Option<u32>,
    precision: Option<u32>,
    min: Option<f64>,
    max: Option<f64>,
    integer: Option<bool>,
    transforms: Option<Value>,
    computed: Option<bool>,
    compute_expression: Option<String>,
    sql_expression: Option<String>,
    display_rules: Option<Value>,
    target_entity: Option<String>,
}

impl Field {
    /// Resolves the PHP entity class backing a `type: "table"` child field.
    ///
    /// An explicit `targetEntity` always wins. Otherwise the class is derived
    /// from the child resource name (e.g. `order_items` -> `OrderItem`) so
    /// callers only need to pass `resource`, matching the resource header.
    fn resolved_target_entity(&self) -> Option<String> {
        if let Some(target) = self
            .target_entity
            .as_deref()
            .map(str::trim)
            .filter(|target| !target.is_empty())
        {
            return Some(target.to_string());
        }

        let resource_name = match &self.resource {
            Some(Value::String(name)) => name.trim(),
            Some(Value::Object(map)) => map
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim(),
            _ => "",
        }
        .trim_matches('/')
        .trim();
        if resource_name.is_empty() {
            return None;
        }

        Some(pascal(&singular(resource_name)))
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResourceAction {
    permission: Option<String>,
    action_expression: Option<String>,
    label_expression: Option<String>,
    confirm: Option<bool>,
    /// Custom JavaScript executed with an injected helper API when the action runs.
    handler: Option<String>,
    /// Optional popup/form configuration that opens a dialog and submits to a custom endpoint.
    form: Option<ActionForm>,
}

/// Configuration for a reusable popup/form row action.
///
/// The action opens a modal built from `fields`, pre-filled with optional
/// static `values`, and submits to `endpoint` (defaulting to a POST against
/// the current resource record).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ActionForm {
    title: Option<String>,
    /// Custom endpoint template; supports `{id}` which is replaced with the record id.
    endpoint: Option<String>,
    /// HTTP method for the submission (defaults to POST).
    method: Option<String>,
    submit_label: Option<String>,
    success_message: Option<String>,
    /// Form fields, using the same field definition shape as resource fields.
    fields: Option<Vec<Field>>,
    /// Static default values keyed by field name.
    values: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Resource {
    name: String,
    endpoint: String,
    fields: Vec<Field>,
    title_expression: Option<String>,
    actions: Option<Vec<ResourceAction>>,
}

pub struct SyncSchemaTool {
    parameters: Map<String, Value>,
}

const RESERVED_COLUMN_NAMES: [&str; 15] = [
    "desc", "asc", "order", "group", "key", "index", "select", "insert", "update", "delete",
    "table", "from", "where", "by", "limit",
];

impl SyncSchemaTool {
    pub fn new() -> Self {
        Self {
            parameters: json!({
                "type": "object",
                "properties": {
                    "resources": {
                        "type": "array",
                        "description": "Full resource definitions to create or completely replace. Use this for new resources. For incremental changes to existing resources, prefer `operations` instead.",
                        "items": { "type": "object" }
                    },
                    "operations": {
                        "type": "array",
                        "description": "Incremental edits to existing resources, applied in order. Use this to add/remove/update/move fields or change resource-level properties without re-emitting the full resource.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "action": {
                                    "type": "string",
                                    "enum": [
                                        "addField",
                                        "updateField",
                                        "removeField",
                                        "moveField",
                                        "updateResource",
                                        "removeResource"
                                    ],
                                    "description": "The edit to perform."
                                },
                                "resource": {
                                    "type": "string",
                                    "description": "Name of the existing resource to edit."
                                },
                                "field": {
                                    "description": "For `addField`: a full field definition object (name + type + properties). For `updateField`, `removeField`, and `moveField`: the existing field name as a string."
                                },
                                "changes": {
                                    "type": "object",
                                    "description": "For `updateField` / `updateResource`: camelCase properties to set. A `null` value removes the property."
                                },
                                "rename": {
                                    "type": "string",
                                    "description": "For `updateField`: the new field name. Renaming drops and re-adds the database column."
                                },
                                "before": {
                                    "type": "string",
                                    "description": "For `addField` / `moveField`: insert before this field."
                                },
                                "after": {
                                    "type": "string",
                                    "description": "For `addField` / `moveField`: insert after this field."
                                },
                                "index": {
                                    "type": "number",
                                    "description": "For `addField` / `moveField`: zero-based insertion index. Only one of `index`, `before`, or `after` may be used."
                                }
                            },
                            "required": ["action", "resource"]
                        }
                    },
                    "projectPath": { "type": "string" }
                }
            })
            .as_object()
            .unwrap()
            .clone(),
        }
    }
}
impl Default for SyncSchemaTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for SyncSchemaTool {
    fn name(&self) -> &str {
        "sync_schema"
    }
    fn description(&self) -> &str {
        "Creates or edits resources and generates React frontend resources and Symfony backend PHP entities, then runs migrations. Use `resources` to create or fully replace resources. Use `operations` for incremental edits (add/update/remove/move fields, update resource properties, remove resources) without re-emitting the whole resource. For `type: \"table\"` fields, pass the child resource name in `resource` (e.g. \"order_items\"); the child PHP entity class (e.g. OrderItem) is derived automatically, so `targetEntity` is not required."
    }
    fn parameters(&self) -> &Map<String, Value> {
        &self.parameters
    }
    async fn execute(&self, args: Value) -> LlmResult<Value> {
        let project = args
            .get("projectPath")
            .and_then(Value::as_str)
            .filter(|path| !path.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("content").join("dummy-project"));

        let full_resources: Vec<Value> = match args.get("resources") {
            Some(Value::Array(items)) => items.clone(),
            Some(value @ Value::Object(_)) => vec![value.clone()],
            _ => Vec::new(),
        };
        let operations: Vec<Value> = match args.get("operations") {
            Some(Value::Array(items)) => items.clone(),
            Some(value @ Value::Object(_)) => vec![value.clone()],
            _ => Vec::new(),
        };

        if full_resources.is_empty() && operations.is_empty() {
            return Err(
                "Provide `resources` (full definitions) and/or `operations` (incremental edits)."
                    .into(),
            );
        }

        let mut schema_resources = read_schema_resources(&project);
        let mut affected: Vec<String> = Vec::new();
        let mut removed: Vec<String> = Vec::new();

        for new_resource in &full_resources {
            let name = resource_name(new_resource)
                .ok_or("Every resource in `resources` must have a non-empty `name`.")?
                .to_string();
            upsert_resource(&mut schema_resources, &name, new_resource.clone());
            push_unique(&mut affected, &name);
        }

        for (position, operation) in operations.iter().enumerate() {
            let outcome = apply_operation(&mut schema_resources, operation)
                .map_err(|error| format!("Operation #{} failed: {}", position + 1, error))?;
            match outcome {
                OperationOutcome::Touched(name) => push_unique(&mut affected, &name),
                OperationOutcome::Removed(name) => {
                    affected.retain(|existing| !existing.eq_ignore_ascii_case(&name));
                    push_unique(&mut removed, &name);
                }
            }
        }

        let mut generated: Vec<Resource> = Vec::with_capacity(affected.len());
        for name in &affected {
            let index = find_resource_index(&schema_resources, name).ok_or_else(|| {
                format!("Resource '{name}' disappeared while applying schema operations.")
            })?;
            let resource: Resource = serde_json::from_value(schema_resources[index].clone())
                .map_err(|error| {
                    format!("Resource '{name}' is not a valid resource definition: {error}")
                })?;
            for field in &resource.fields {
                if RESERVED_COLUMN_NAMES.contains(&field.name.as_str()) {
                    return Err(format!(
                        "Column name '{}' on resource '{}' is reserved and cannot be used. Reserved names: {}. Please choose a different column name.",
                        field.name,
                        resource.name,
                        RESERVED_COLUMN_NAMES.join(", ")
                    )
                    .into());
                }
                if field.field_type == "table" && field.resolved_target_entity().is_none() {
                    return Err(format!(
                        "Table field '{}' on resource '{}' needs a `resource` property naming the child resource (e.g. resource: \"order_items\"). It may also be passed in the same call; the child PHP entity name is derived automatically.",
                        field.name, resource.name
                    )
                    .into());
                }
            }
            generated.push(resource);
        }

        let mut frontend = Vec::new();
        let mut backend = Vec::new();
        for resource in &generated {
            let class = pascal(&resource.name);
            let frontend_dir = if project.join("frontend/src/resources").exists() {
                project.join("frontend/src/resources")
            } else {
                project.join("frontend/resources")
            };
            std::fs::create_dir_all(&frontend_dir)?;
            let fp = frontend_dir.join(format!("{}.ts", resource.name));
            std::fs::write(&fp, frontend_code(resource, &class))?;
            frontend.push(fp);

            let singular_name = singular(&resource.name);
            let class = pascal(&singular_name);
            let dir = project.join("backend/src/Entity");
            std::fs::create_dir_all(&dir)?;
            let bp = dir.join(format!("{}.php", class));
            std::fs::write(&bp, backend_code(resource, &class))?;
            backend.push(bp);
        }

        let schema_path = write_schema_resources(&project, &schema_resources)?;

        if !generated.is_empty() {
            run_migrations(&project)?;
        }

        let mut message = String::from("Schema sync completed successfully.");
        if !removed.is_empty() {
            message.push_str(&format!(
                " Removed from schema (generated files were left in place): {}.",
                removed.join(", ")
            ));
        }

        Ok(json!({
            "success": true,
            "message": message,
            "details": {
                "resources": generated.iter().map(|r| r.name.clone()).collect::<Vec<_>>(),
                "removedResources": removed,
                "frontendResourcePaths": frontend,
                "backendEntityPaths": backend,
                "schemaPath": schema_path,
                "timestamp": timestamp()
            }
        }))
    }
}

/// Result of applying a single incremental schema operation.
#[derive(Debug)]
enum OperationOutcome {
    /// The resource was edited and must be regenerated.
    Touched(String),
    /// The resource was removed from the schema.
    Removed(String),
}

fn resource_name(resource: &Value) -> Option<&str> {
    resource
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
}

fn push_unique(names: &mut Vec<String>, name: &str) {
    if !names.iter().any(|existing| existing.eq_ignore_ascii_case(name)) {
        names.push(name.to_string());
    }
}

fn find_resource_index(resources: &[Value], name: &str) -> Option<usize> {
    resources.iter().position(|resource| {
        resource_name(resource)
            .map(|existing| existing.eq_ignore_ascii_case(name))
            .unwrap_or(false)
    })
}

fn require_resource_index(resources: &[Value], name: &str) -> Result<usize, String> {
    find_resource_index(resources, name).ok_or_else(|| {
        let available = resources
            .iter()
            .filter_map(resource_name)
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "resource '{name}' does not exist. Available resources: {}",
            if available.is_empty() {
                "<none>".to_string()
            } else {
                available
            }
        )
    })
}

/// Replaces the resource with the same name, or appends it when new.
fn upsert_resource(resources: &mut Vec<Value>, name: &str, resource: Value) {
    if let Some(index) = find_resource_index(resources, name) {
        resources[index] = resource;
    } else {
        resources.push(resource);
    }
}

fn field_name(field: &Value) -> Option<&str> {
    field
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
}

fn field_index(resource: &Value, name: &str) -> Option<usize> {
    resource
        .get("fields")
        .and_then(Value::as_array)?
        .iter()
        .position(|field| {
            field_name(field)
                .map(|existing| existing.eq_ignore_ascii_case(name))
                .unwrap_or(false)
        })
}

fn require_fields_mut(resource: &mut Value) -> Result<&mut Vec<Value>, String> {
    if !resource.is_object() {
        return Err("resource definition must be a JSON object".to_string());
    }
    let object = resource.as_object_mut().unwrap();
    let entry = object
        .entry("fields")
        .or_insert_with(|| Value::Array(Vec::new()));
    entry
        .as_array_mut()
        .ok_or_else(|| "resource `fields` must be an array".to_string())
}

/// Applies `changes` on top of `target`. A `null` value deletes the property.
fn apply_changes(target: &mut Map<String, Value>, changes: &Map<String, Value>) {
    for (key, value) in changes {
        if value.is_null() {
            target.remove(key);
        } else {
            target.insert(key.clone(), value.clone());
        }
    }
}

fn resolve_insert_index(
    operation: &Map<String, Value>,
    fields: &[Value],
) -> Result<usize, String> {
    let index = operation.get("index").and_then(Value::as_u64);
    let before = operation.get("before").and_then(Value::as_str);
    let after = operation.get("after").and_then(Value::as_str);

    let provided = usize::from(index.is_some()) + usize::from(before.is_some()) + usize::from(after.is_some());
    if provided > 1 {
        return Err("use only one of `index`, `before`, or `after`".to_string());
    }

    if let Some(index) = index {
        return Ok((index as usize).min(fields.len()));
    }

    if let Some(before) = before {
        return fields
            .iter()
            .position(|field| {
                field_name(field)
                    .map(|name| name.eq_ignore_ascii_case(before))
                    .unwrap_or(false)
            })
            .ok_or_else(|| {
                format!("cannot insert before '{before}' because that field does not exist")
            });
    }

    if let Some(after) = after {
        return fields
            .iter()
            .position(|field| {
                field_name(field)
                    .map(|name| name.eq_ignore_ascii_case(after))
                    .unwrap_or(false)
            })
            .map(|position| position + 1)
            .ok_or_else(|| {
                format!("cannot insert after '{after}' because that field does not exist")
            });
    }

    Ok(fields.len())
}

/// Applies one incremental edit operation to the schema resource list.
fn apply_operation(
    resources: &mut Vec<Value>,
    operation: &Value,
) -> Result<OperationOutcome, String> {
    let object = operation
        .as_object()
        .ok_or("each operation must be a JSON object")?;
    let action = object
        .get("action")
        .and_then(Value::as_str)
        .ok_or("every operation requires an `action`")?;
    let resource_name = object
        .get("resource")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .ok_or("every operation requires a `resource` name")?
        .to_string();

    if action == "removeResource" {
        let index = require_resource_index(resources, &resource_name)?;
        resources.remove(index);
        return Ok(OperationOutcome::Removed(resource_name));
    }

    let index = require_resource_index(resources, &resource_name)?;

    match action {
        "addField" => {
            let field = object
                .get("field")
                .ok_or("`addField` requires a `field` object")?;
            if !field.is_object() {
                return Err("`addField` expects `field` to be a field definition object".into());
            }
            let name = field_name(field)
                .ok_or("`addField` requires `field.name`")?
                .to_string();
            if field.get("type").and_then(Value::as_str).is_none() {
                return Err(format!("field '{name}' requires a `type`"));
            }
            if field_index(&resources[index], &name).is_some() {
                return Err(format!(
                    "field '{name}' already exists on resource '{resource_name}'; use `updateField` instead"
                ));
            }
            let fields = require_fields_mut(&mut resources[index])?;
            let position = resolve_insert_index(object, fields)?;
            fields.insert(position, field.clone());
        }
        "updateField" => {
            let name = object
                .get("field")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .ok_or("`updateField` requires `field` to be the existing field name")?
                .to_string();
            let changes = object
                .get("changes")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let rename = object
                .get("rename")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty());
            if changes.is_empty() && rename.is_none() {
                return Err(
                    "`updateField` requires at least one of `changes` or `rename`".into(),
                );
            }
            if changes.get("name").is_some_and(Value::is_null) {
                return Err("`updateField` cannot remove `name`; use `rename` instead".into());
            }
            if changes.get("type").is_some_and(Value::is_null) {
                return Err("`updateField` cannot remove `type`; every field needs a type".into());
            }

            let position = field_index(&resources[index], &name)
                .ok_or_else(|| format!("field '{name}' does not exist on resource '{resource_name}'"))?;

            let new_name = rename
                .map(str::to_string)
                .or_else(|| changes.get("name").and_then(Value::as_str).map(str::to_string))
                .unwrap_or_else(|| name.clone());
            if !new_name.eq_ignore_ascii_case(&name) {
                if let Some(existing) = field_index(&resources[index], &new_name) {
                    if existing != position {
                        return Err(format!(
                            "cannot rename field '{name}' to '{new_name}' because it already exists"
                        ));
                    }
                }
            }

            let fields = require_fields_mut(&mut resources[index])?;
            let field = fields[position]
                .as_object_mut()
                .ok_or_else(|| format!("field '{name}' must be a JSON object"))?;
            apply_changes(field, &changes);
            if rename.is_some() {
                field.insert("name".to_string(), Value::String(new_name));
            }
        }
        "removeField" => {
            let name = object
                .get("field")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .ok_or("`removeField` requires `field` to be the field name")?;
            let position = field_index(&resources[index], name)
                .ok_or_else(|| format!("field '{name}' does not exist on resource '{resource_name}'"))?;
            require_fields_mut(&mut resources[index])?.remove(position);
        }
        "moveField" => {
            let name = object
                .get("field")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .ok_or("`moveField` requires `field` to be the field name")?;
            let position = field_index(&resources[index], name)
                .ok_or_else(|| format!("field '{name}' does not exist on resource '{resource_name}'"))?;
            let fields = require_fields_mut(&mut resources[index])?;
            let field = fields.remove(position);
            let position = resolve_insert_index(object, fields)?;
            fields.insert(position, field);
        }
        "updateResource" => {
            let changes = object
                .get("changes")
                .and_then(Value::as_object)
                .ok_or("`updateResource` requires a `changes` object")?;
            if changes.contains_key("name") {
                return Err(
                    "resource renaming is not supported; create the new resource and remove the old one"
                        .into(),
                );
            }
            if changes.contains_key("fields") {
                return Err(
                    "use `addField`, `updateField`, `removeField`, and `moveField` to change fields"
                        .into(),
                );
            }
            let resource = resources[index]
                .as_object_mut()
                .ok_or_else(|| format!("resource '{resource_name}' must be a JSON object"))?;
            apply_changes(resource, changes);
        }
        other => {
            return Err(format!(
                "unknown action '{other}'. Supported actions: addField, updateField, removeField, moveField, updateResource, removeResource"
            ))
        }
    }

    Ok(OperationOutcome::Touched(resource_name))
}

fn run_migrations(project: &Path) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let backend_dir = project.join("backend");
    for command in [
        vec!["bin/console", "make:migration"],
        vec![
            "bin/console",
            "doctrine:migrations:migrate",
            "--no-interaction",
        ],
    ] {
        let mut cmd = crate::llm::symfony::new_php_command();
        cmd.args(&command)
            .current_dir(&backend_dir)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        let output = cmd.output().map_err(|e| {
            format!(
                "Failed to execute PHP migration command 'php {}': {e}. Ensure PHP is installed and accessible.",
                command.join(" ")
            )
        })?;

        if !output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let details = if !stderr.trim().is_empty() {
                stderr.trim()
            } else {
                stdout.trim()
            };
            return Err(format!(
                "Migration command failed with {}: php {}\n{}",
                output.status,
                command.join(" "),
                details
            )
            .into());
        }
    }
    Ok(())
}

pub fn schema_file_path(project: &Path) -> PathBuf {
    let p1 = project.join("schemas.json");
    if p1.exists() {
        return p1;
    }
    let p2 = project.join("schema.json");
    if p2.exists() {
        return p2;
    }
    p1
}

/// Reads the resource definitions stored in the project's schema file.
///
/// The file may contain either a bare array of resources or an object with a
/// `resources` array. A missing or unreadable file yields an empty list so that
/// callers can treat "no schema yet" and "new project" identically.
pub fn read_schema_resources(project: &Path) -> Vec<Value> {
    let schema_path = schema_file_path(project);
    let content = match std::fs::read_to_string(&schema_path) {
        Ok(content) => content,
        Err(_) => return Vec::new(),
    };
    let parsed: Value = match serde_json::from_str(&content) {
        Ok(value) => value,
        Err(_) => return Vec::new(),
    };
    match parsed {
        Value::Array(arr) => arr,
        Value::Object(map) => match map.get("resources") {
            Some(Value::Array(arr)) => arr.clone(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// Writes the full list of resource definitions to the project's schema file.
pub fn write_schema_resources(
    project: &Path,
    resources: &[Value],
) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    std::fs::create_dir_all(project)?;
    let schema_path = schema_file_path(project);
    let schema_doc = json!({
        "resources": resources
    });
    std::fs::write(&schema_path, serde_json::to_string_pretty(&schema_doc)?)?;
    Ok(schema_path)
}

/// Merges `resources_value` into the project's schema file.
///
/// Kept as a public convenience helper; `SyncSchemaTool` itself edits an
/// in-memory list and calls [`write_schema_resources`] once so partial edits
/// are never lost.
#[allow(dead_code)]
pub fn save_schemas(
    project: &Path,
    resources_value: Option<&Value>,
) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    let new_resources: Vec<Value> = match resources_value {
        Some(Value::Array(arr)) => arr.clone(),
        Some(val @ Value::Object(_)) => vec![val.clone()],
        _ => Vec::new(),
    };

    let mut existing_resources = read_schema_resources(project);

    for new_res in new_resources {
        let new_name = resource_name(&new_res).unwrap_or("");
        if let Some(pos) = find_resource_index(&existing_resources, new_name) {
            existing_resources[pos] = new_res;
        } else {
            existing_resources.push(new_res);
        }
    }

    write_schema_resources(project, &existing_resources)
}

fn js(s: &str) -> String {
    serde_json::to_string(s).unwrap()
}
fn frontend_code(r: &Resource, _class: &str) -> String {
    let fields = r
        .fields
        .iter()
        .map(field_js)
        .collect::<Vec<_>>()
        .join(",\n");
    format!("import defineResource from '../utils/defineResource';\nimport field from '../utils/field';\n\nexport default defineResource({{\n  name: {},\n  endpoint: {},\n  fields: [\n{}\n  ],\n{}  titleExpression: {}\n}});\n", js(&r.name), js(&r.endpoint), fields, actions_js(r), js(r.title_expression.as_deref().unwrap_or("{id}")))
}

fn actions_js(r: &Resource) -> String {
    let actions = match &r.actions {
        Some(actions) if !actions.is_empty() => actions,
        _ => return String::new(),
    };
    let items = actions
        .iter()
        .map(|a| {
            let mut s = String::from("    {\n");
            if let Some(permission) = &a.permission {
                s.push_str(&format!("      permission: {},\n", js(permission)));
            }
            if let Some(expression) = &a.action_expression {
                s.push_str(&format!("      actionExpression: {},\n", js(expression)));
            }
            if let Some(expression) = &a.label_expression {
                s.push_str(&format!("      labelExpression: {},\n", js(expression)));
            }
            if a.confirm.unwrap_or(false) {
                s.push_str("      confirm: true,\n");
            }
            if let Some(handler) = &a.handler {
                s.push_str(&format!("      handler: {},\n", js(handler)));
            }
            if let Some(form) = &a.form {
                s.push_str(&format!("      form: {},\n", action_form_js(form)));
            }
            s.push_str("    }");
            s
        })
        .collect::<Vec<_>>()
        .join(",\n");
    format!("  actions: [\n{}\n  ],\n", items)
}

/// Serializes an action form configuration into an object literal.
fn action_form_js(form: &ActionForm) -> String {
    let mut s = String::from("{\n");
    if let Some(title) = &form.title {
        s.push_str(&format!("        title: {},\n", js(title)));
    }
    if let Some(endpoint) = &form.endpoint {
        s.push_str(&format!("        endpoint: {},\n", js(endpoint)));
    }
    if let Some(method) = &form.method {
        s.push_str(&format!("        method: {},\n", js(method)));
    }
    if let Some(label) = &form.submit_label {
        s.push_str(&format!("        submitLabel: {},\n", js(label)));
    }
    if let Some(message) = &form.success_message {
        s.push_str(&format!("        successMessage: {},\n", js(message)));
    }
    if let Some(values) = &form.values {
        s.push_str(&format!(
            "        values: {},\n",
            serde_json::to_string(values).unwrap_or_else(|_| "{}".to_string())
        ));
    }
    if let Some(fields) = &form.fields {
        let rendered = fields
            .iter()
            .map(|f| {
                let js_field = field_js(f);
                // Indent the field literal to sit inside the form object.
                js_field
                    .lines()
                    .map(|line| format!("          {line}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .collect::<Vec<_>>()
            .join(",\n");
        s.push_str(&format!("        fields: [\n{}\n        ],\n", rendered));
    }
    s.push_str("      }");
    s
}
fn field_js(f: &Field) -> String {
    let mut s = format!("    field.{}({})", f.field_type, js(&f.name));
    macro_rules! call {
        ($n:expr,$v:expr) => {
            if let Some(v) = $v {
                s.push_str(&format!("\n      .{}({})", $n, js(v)));
            }
        };
    }
    macro_rules! flag {
        ($n:expr,$v:expr) => {
            if $v.unwrap_or(false) {
                s.push_str(&format!("\n      .{}()", $n));
            }
        };
    }
    call!("label", f.label.as_deref());
    call!("visibleWhen", f.visible_when.as_deref());
    call!("trueLabel", f.true_label.as_deref());
    call!("falseLabel", f.false_label.as_deref());
    call!("source", f.source.as_deref());
    call!("accept", f.accept.as_deref());
    call!("map", f.map.as_deref());
    if let Some(v) = &f.resource {
        s.push_str(&format!("\n      .resource({})", v));
    }
    if let Some(v) = &f.columns {
        s.push_str(&format!("\n      .columns({})", v));
    }
    if let Some(opts) = &f.options {
        let entries = opts
            .iter()
            .map(|(k, v)| format!("{}: {}", js(k), serde_json::to_string(v).unwrap_or_default()))
            .collect::<Vec<_>>()
            .join(", ");
        s.push_str(&format!("\n      .options({{ {} }})", entries));
    }
    if let Some(v) = &f.default {
        s.push_str(&format!("\n      .default({})", serde_json::to_string(v).unwrap_or_default()));
    }
    if let Some(v) = &f.messages {
        s.push_str(&format!("\n      .messages({})", serde_json::to_string(v).unwrap_or_default()));
    }
    if f.field_type != "table" {
        flag!("table", f.table);
    }
    flag!("form", f.form);
    flag!("detail", f.detail);
    if let Some(v) = f.length {
        s.push_str(&format!("\n      .length({})", v));
    }
    if let Some(v) = f.min_size {
        s.push_str(&format!("\n      .minSize({})", v));
    }
    if let Some(v) = f.max_size {
        s.push_str(&format!("\n      .maxSize({})", v));
    }
    if let Some(v) = f.min {
        s.push_str(&format!("\n      .min({})", v));
    }
    if let Some(v) = f.max {
        s.push_str(&format!("\n      .max({})", v));
    }
    if let Some(v) = f.scale {
        s.push_str(&format!("\n      .scale({})", v));
    }
    if let Some(v) = f.precision {
        s.push_str(&format!("\n      .precision({})", v));
    }
    if let Some(v) = &f.transforms {
        s.push_str(&format!("\n      .transforms({})", serde_json::to_string(v).unwrap_or_default()));
    }
    if let Some(v) = &f.display_rules {
        s.push_str(&format!("\n      .displayRules({})", serde_json::to_string(v).unwrap_or_default()));
    }
    call!("computeExpression", f.compute_expression.as_deref());
    call!("sqlExpression", f.sql_expression.as_deref());
    flag!("computed", f.computed);
    flag!("integer", f.integer);
    flag!("required", f.required);
    flag!("email", f.email);
    flag!("phone", f.phone);
    flag!("unique", f.unique);
    flag!("sortable", f.sortable);
    flag!("searchable", f.searchable);
    flag!("filterable", f.filterable);
    flag!("password", f.password);
    s
}

fn backend_code(r: &Resource, class: &str) -> String {
    let has_computed_fields = r
        .fields
        .iter()
        .any(|f| f.computed.unwrap_or(false) || f.sql_expression.is_some());
    let mut import_lines = vec![
        "use Doctrine\\ORM\\Mapping as ORM;".to_string(),
        "use Doctrine\\DBAL\\Types\\Types;".to_string(),
        "use App\\Resource\\ResourceEntity;".to_string(),
        "use App\\Resource\\Attribute\\Form;".to_string(),
        "use App\\Resource\\Attribute\\Phone as PhoneAttribute;".to_string(),
        "use App\\Validator\\Phone as PhoneConstraint;".to_string(),
        "use App\\Validator\\OneOf as OneOfConstraint;".to_string(),
        "use App\\Validator\\Time as TimeConstraint;".to_string(),
        "use App\\Validator\\Unique as UniqueConstraint;".to_string(),
        "use Symfony\\Component\\Validator\\Constraints as Assert;".to_string(),
    ];

    let has_table_relations = r
        .fields
        .iter()
        .any(|f| f.field_type == "table" && f.resolved_target_entity().is_some());
    if has_table_relations {
        import_lines.push("use Doctrine\\Common\\Collections\\ArrayCollection;".to_string());
        import_lines.push("use Doctrine\\Common\\Collections\\Collection;".to_string());
        import_lines.push("use App\\Resource\\MapField;".to_string());
    }

    if has_computed_fields {
        import_lines.push("use App\\Resource\\Attribute\\Computed;".to_string());
    }

    let mut imported_entities = std::collections::BTreeSet::new();
    for f in &r.fields {
        if f.field_type == "table" {
            if let Some(target_class) = f.resolved_target_entity() {
                if target_class != class && !imported_entities.contains(&target_class) {
                    import_lines.push(format!("use App\\Entity\\{};", target_class));
                    imported_entities.insert(target_class);
                }
            }
        }
    }

    let imports = import_lines.join("\n");
    let mut collection_inits = Vec::new();

    let props = r
        .fields
        .iter()
        .map(|f| {
            if f.computed.unwrap_or(false) {
                let mut computed_args = Vec::new();
                if let Some(ref expr) = f.sql_expression {
                    let escaped = expr.replace('\\', "\\\\").replace('\'', "\\'");
                    computed_args.push(format!("sqlExpression: '{}'", escaped));
                }
                if let Some(ref expr) = f.compute_expression {
                    let escaped = expr.replace('\\', "\\\\").replace('\'', "\\'");
                    computed_args.push(format!("computeExpression: '{}'", escaped));
                }
                let computed_annotation = if computed_args.is_empty() {
                    "    #[Computed]\n".to_string()
                } else {
                    format!("    #[Computed({})]\n", computed_args.join(", "))
                };

                let is_decimal = f.field_type == "decimal";
                let column_opts = {
                    let mut options = Vec::new();
                    if is_decimal {
                        options.push("type: Types::DECIMAL".to_string());
                        let precision = f.precision.unwrap_or(10);
                        let scale = f.scale.unwrap_or(2);
                        options.push(format!("precision: {}", precision));
                        options.push(format!("scale: {}", scale));
                    }
                    options.push("nullable: true".to_string());
                    format!("({})", options.join(", "))
                };

                return format!(
                    "{}{}    #[Form(type: '{}')]\n    #[ORM\\Column{}]\n    public ?{} ${} = null;",
                    computed_annotation,
                    if f.phone.unwrap_or(false) || f.field_type == "phone" {
                        "    #[PhoneAttribute]\n"
                    } else {
                        ""
                    },
                    f.field_type,
                    column_opts,
                    php_type(&f.field_type),
                    f.name
                );
            }
            if f.field_type == "table" {
                if let Some(target_class) = f.resolved_target_entity() {
                    let mapped_by = f.map.as_deref().unwrap_or("orderId");
                    collection_inits.push(format!("        $this->{} = new ArrayCollection();", f.name));
                    return format!(
                        "    #[MapField(field: '{}', targetEntity: {}::class)]\n    public Collection ${};",
                        mapped_by, target_class, f.name
                    );
                }
            }
            let mut asserts = String::new();
            if f.required.unwrap_or(false) {
                asserts.push_str("    #[Assert\\NotBlank]\n");
            }
            if f.email.unwrap_or(false) || f.field_type == "email" {
                asserts.push_str(
                    "    #[Assert\\Email(\n        message: 'The email {{ value }} is not a valid email.',\n    )]\n",
                );
            }
            if f.phone.unwrap_or(false) || f.field_type == "phone" {
                asserts.push_str(
                    "    #[PhoneConstraint(\n        message: 'The value {{ value }} is not a valid phone number. It must be in E.164 format (e.g. +14155552671).',\n    )]\n",
                );
            }
            if f.unique.unwrap_or(false) {
                let unique_message = f
                    .messages
                    .as_ref()
                    .and_then(|m| m.get("unique"))
                    .and_then(Value::as_str)
                    .map(|s| format!("\n        message: '{}',", s.replace('\'', "\\'")))
                    .unwrap_or_default();
                asserts.push_str(&format!(
                    "    #[UniqueConstraint{}]\n",
                    if unique_message.is_empty() {
                        String::new()
                    } else {
                        format!("({}\n    )", unique_message)
                    }
                ));
            }
            if f.field_type == "time" {
                asserts.push_str(
                    "    #[TimeConstraint(\n        message: 'The value {{ value }} is not a valid time. It must be in HH:MM:SS format (e.g. 14:30:00).',\n    )]\n",
                );
            }
            if let Some(opts) = &f.options {
                let choices = opts
                    .keys()
                    .map(|k| format!("'{}'", k.replace('\'', "\\'")))
                    .collect::<Vec<_>>()
                    .join(", ");
                asserts.push_str(&format!(
                    "    #[OneOfConstraint(choices: [{}])]\n",
                    choices
                ));
            }
            if f.min_size.is_some() || f.max_size.is_some() {
                let mut constraint = String::from("    #[Assert\\Length(\n");
                if let Some(min) = f.min_size {
                    constraint.push_str(&format!(
                        "        min: {},\n        minMessage: 'The value must be at least {{{{ limit }}}} characters long',\n",
                        min
                    ));
                }
                if let Some(max) = f.max_size {
                    constraint.push_str(&format!(
                        "        max: {},\n        maxMessage: 'The value can be at most {{{{ limit }}}} characters',\n",
                        max
                    ));
                }
                constraint.push_str("    )]\n");
                asserts.push_str(&constraint);
            }
            if let Some(min) = f.min {
                if min == 0.0 {
                    asserts.push_str("    #[Assert\\PositiveOrZero]\n");
                } else {
                    asserts.push_str(&format!(
                        "    #[Assert\\GreaterThanOrEqual({})]\n",
                        min
                    ));
                }
            }
            if let Some(max) = f.max {
                asserts.push_str(&format!(
                    "    #[Assert\\LessThanOrEqual({})]\n",
                    max
                ));
            }
            if f.integer.unwrap_or(false) {
                asserts.push_str("    #[Assert\\Type('integer')]\n");
            }
            let is_decimal = f.field_type == "decimal";
            format!(
                "{}    #[Form(type: '{}')]{}\n    #[ORM\\Column{}]\n    public ?{} ${} = null;",
                asserts,
                f.field_type,
                if f.phone.unwrap_or(false) || f.field_type == "phone" {
                    "\n    #[PhoneAttribute]"
                } else {
                    ""
                },
                {
                    let mut options = Vec::new();
                    if is_decimal {
                        options.push("type: Types::DECIMAL".to_string());
                        let precision = f.precision.unwrap_or(10);
                        let scale = f.scale.unwrap_or(2);
                        options.push(format!("precision: {}", precision));
                        options.push(format!("scale: {}", scale));
                    }
                    if f.unique.unwrap_or(false) {
                        options.push("unique: true".to_string());
                    }
                    if !f.required.unwrap_or(false) {
                        options.push("nullable: true".to_string());
                    }
                    if options.is_empty() {
                        String::new()
                    } else {
                        format!("({})", options.join(", "))
                    }
                },
                php_type(&f.field_type),
                f.name
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    let constructor = if collection_inits.is_empty() {
        String::new()
    } else {
        format!(
            "\n\n    public function __construct()\n    {{\n{}\n    }}",
            collection_inits.join("\n")
        )
    };

    format!("<?php\n\nnamespace App\\Entity;\n\n{}\n\n#[ORM\\Entity()]\n#[ORM\\Table(name: '{}')]\nclass {} extends ResourceEntity\n{{\n    #[ORM\\Id]\n    #[ORM\\GeneratedValue]\n    #[ORM\\Column]\n    public ?int $id = null;\n\n{}{}\n\n    public function getTitle(): string\n    {{\n        return (string) $this->id;\n    }}\n}}\n",imports,r.name,class,props,constructor)
}
fn php_type(t: &str) -> &str {
    match t {
        "number" | "foreign" => "int",
        "decimal" => "string",
        "boolean" => "bool",
        "time" => "string",
        "table" => "array",
        _ => "string",
    }
}
fn pascal(s: &str) -> String {
    s.split(['_', '-', ' '])
        .filter(|x| !x.is_empty())
        .map(|x| {
            let mut c = x.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect()
}
fn singular(s: &str) -> String {
    if s.ends_with("ies") && s.len() > 3 {
        format!("{}y", &s[..s.len() - 3])
    } else if s.ends_with("sses")
        || s.ends_with("shes")
        || s.ends_with("ches")
        || s.ends_with("xes")
        || s.ends_with("zes")
    {
        s[..s.len() - 2].to_string()
    } else if s.ends_with("uses") && s.len() > 4 {
        // statuses -> status, census stays as-is via the check below.
        s[..s.len() - 2].to_string()
    } else if s.ends_with("ss") || s.ends_with("us") || s.ends_with("is") {
        s.to_string()
    } else if s.ends_with('s') {
        s[..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}
fn timestamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema_fixture() -> Vec<Value> {
        vec![json!({
            "name": "products",
            "endpoint": "/products",
            "fields": [
                { "name": "name", "type": "text", "label": "Name" },
                { "name": "price", "type": "decimal", "label": "Price" }
            ]
        })]
    }

    fn field_names(resources: &[Value]) -> Vec<String> {
        resources[0]["fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["name"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn adds_field_at_end_by_default() {
        let mut resources = schema_fixture();
        apply_operation(
            &mut resources,
            &json!({
                "action": "addField",
                "resource": "products",
                "field": { "name": "sku", "type": "text" }
            }),
        )
        .unwrap();
        assert_eq!(field_names(&resources), ["name", "price", "sku"]);
    }

    #[test]
    fn adds_field_before_and_after_existing_fields() {
        let mut resources = schema_fixture();
        apply_operation(
            &mut resources,
            &json!({
                "action": "addField",
                "resource": "products",
                "field": { "name": "sku", "type": "text" },
                "before": "name"
            }),
        )
        .unwrap();
        assert_eq!(field_names(&resources), ["sku", "name", "price"]);

        apply_operation(
            &mut resources,
            &json!({
                "action": "addField",
                "resource": "products",
                "field": { "name": "stock", "type": "number" },
                "after": "price"
            }),
        )
        .unwrap();
        assert_eq!(field_names(&resources), ["sku", "name", "price", "stock"]);
    }

    #[test]
    fn rejects_duplicate_field_name() {
        let mut resources = schema_fixture();
        let error = apply_operation(
            &mut resources,
            &json!({
                "action": "addField",
                "resource": "products",
                "field": { "name": "price", "type": "text" }
            }),
        )
        .unwrap_err();
        assert!(error.contains("already exists"), "{error}");
    }

    #[test]
    fn rejects_multiple_insert_anchors() {
        let mut resources = schema_fixture();
        let error = apply_operation(
            &mut resources,
            &json!({
                "action": "addField",
                "resource": "products",
                "field": { "name": "sku", "type": "text" },
                "before": "name",
                "index": 0
            }),
        )
        .unwrap_err();
        assert!(error.contains("only one of"), "{error}");
    }

    #[test]
    fn updates_field_properties_and_clears_with_null() {
        let mut resources = schema_fixture();
        apply_operation(
            &mut resources,
            &json!({
                "action": "updateField",
                "resource": "products",
                "field": "price",
                "changes": { "required": true, "label": null }
            }),
        )
        .unwrap();
        let price = &resources[0]["fields"][1];
        assert_eq!(price["required"], json!(true));
        assert!(price.get("label").is_none());
    }

    #[test]
    fn renames_field_and_rejects_collision() {
        let mut resources = schema_fixture();
        apply_operation(
            &mut resources,
            &json!({
                "action": "updateField",
                "resource": "products",
                "field": "price",
                "rename": "unitPrice"
            }),
        )
        .unwrap();
        assert_eq!(field_names(&resources), ["name", "unitPrice"]);

        let error = apply_operation(
            &mut resources,
            &json!({
                "action": "updateField",
                "resource": "products",
                "field": "unitPrice",
                "rename": "name"
            }),
        )
        .unwrap_err();
        assert!(error.contains("already exists"), "{error}");
    }

    #[test]
    fn removes_and_moves_fields() {
        let mut resources = schema_fixture();
        apply_operation(
            &mut resources,
            &json!({
                "action": "removeField",
                "resource": "products",
                "field": "name"
            }),
        )
        .unwrap();
        assert_eq!(field_names(&resources), ["price"]);

        apply_operation(
            &mut resources,
            &json!({
                "action": "addField",
                "resource": "products",
                "field": { "name": "sku", "type": "text" }
            }),
        )
        .unwrap();
        apply_operation(
            &mut resources,
            &json!({
                "action": "moveField",
                "resource": "products",
                "field": "sku",
                "before": "price"
            }),
        )
        .unwrap();
        assert_eq!(field_names(&resources), ["sku", "price"]);
    }

    #[test]
    fn updates_resource_properties() {
        let mut resources = schema_fixture();
        apply_operation(
            &mut resources,
            &json!({
                "action": "updateResource",
                "resource": "products",
                "changes": { "titleExpression": "data.name", "endpoint": "/inventory/products" }
            }),
        )
        .unwrap();
        assert_eq!(resources[0]["titleExpression"], json!("data.name"));
        assert_eq!(resources[0]["endpoint"], json!("/inventory/products"));
    }

    #[test]
    fn rejects_resource_rename_and_field_replacement_via_update_resource() {
        let mut resources = schema_fixture();
        let rename_error = apply_operation(
            &mut resources,
            &json!({
                "action": "updateResource",
                "resource": "products",
                "changes": { "name": "items" }
            }),
        )
        .unwrap_err();
        assert!(rename_error.contains("renaming is not supported"));

        let fields_error = apply_operation(
            &mut resources,
            &json!({
                "action": "updateResource",
                "resource": "products",
                "changes": { "fields": [] }
            }),
        )
        .unwrap_err();
        assert!(fields_error.contains("use `addField`"));
    }

    #[test]
    fn removes_resource() {
        let mut resources = schema_fixture();
        resources.push(json!({ "name": "customers", "endpoint": "/customers", "fields": [] }));
        let outcome = apply_operation(
            &mut resources,
            &json!({ "action": "removeResource", "resource": "customers" }),
        )
        .unwrap();
        assert!(matches!(outcome, OperationOutcome::Removed(ref name) if name == "customers"));
        assert_eq!(resources.len(), 1);
    }

    #[test]
    fn unknown_resource_reports_available_names() {
        let mut resources = schema_fixture();
        let error = apply_operation(
            &mut resources,
            &json!({
                "action": "removeField",
                "resource": "orders",
                "field": "name"
            }),
        )
        .unwrap_err();
        assert!(error.contains("does not exist"), "{error}");
        assert!(error.contains("products"), "{error}");
    }

    fn table_field(resource_value: Value) -> Field {
        serde_json::from_value(json!({
            "name": "items",
            "type": "table",
            "resource": resource_value,
            "map": "orderId"
        }))
        .unwrap()
    }

    #[test]
    fn derives_target_entity_from_resource_name() {
        assert_eq!(
            table_field(json!("order_items")).resolved_target_entity().as_deref(),
            Some("OrderItem")
        );
        assert_eq!(
            table_field(json!("invoice_lines")).resolved_target_entity().as_deref(),
            Some("InvoiceLine")
        );
        assert_eq!(
            table_field(json!({ "name": "order_items" }))
                .resolved_target_entity()
                .as_deref(),
            Some("OrderItem")
        );
    }

    #[test]
    fn explicit_target_entity_overrides_resource_name() {
        let field: Field = serde_json::from_value(json!({
            "name": "items",
            "type": "table",
            "resource": "order_items",
            "targetEntity": "LegacyItem"
        }))
        .unwrap();
        assert_eq!(field.resolved_target_entity().as_deref(), Some("LegacyItem"));
    }

    #[test]
    fn table_field_without_resource_has_no_target_entity() {
        let field: Field = serde_json::from_value(json!({
            "name": "items",
            "type": "table"
        }))
        .unwrap();
        assert!(field.resolved_target_entity().is_none());
    }

    #[test]
    fn singularization_keeps_non_plural_words_intact() {
        assert_eq!(singular("order_items"), "order_item");
        assert_eq!(singular("categories"), "category");
        assert_eq!(singular("statuses"), "status");
        assert_eq!(singular("status"), "status");
        assert_eq!(singular("address"), "address");
    }

    #[test]
    fn generates_stored_computed_column_with_attribute() {
        let resource: Resource = serde_json::from_value(json!({
            "name": "sales",
            "endpoint": "/sales",
            "fields": [
                {
                    "name": "totalAmount",
                    "type": "decimal",
                    "scale": 2,
                    "computed": true,
                    "sqlExpression": "(SELECT COALESCE(SUM(i.quantity * i.unitPrice), 0) FROM App\\Entity\\SaleItem i WHERE i.saleId = e.id)"
                }
            ]
        }))
        .unwrap();

        let code = backend_code(&resource, "Sale");
        assert!(code.contains("use App\\Resource\\Attribute\\Computed;"));
        assert!(code.contains("#[Computed(sqlExpression: '(SELECT COALESCE(SUM(i.quantity * i.unitPrice), 0) FROM App\\\\Entity\\\\SaleItem i WHERE i.saleId = e.id)')]"));
        assert!(code.contains("#[ORM\\Column(type: Types::DECIMAL, precision: 10, scale: 2, nullable: true)]"));
        assert!(code.contains("public ?string $totalAmount = null;"));
    }
}
