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
    /// Static button label. `labelExpression` wins when both are present.
    label: Option<String>,
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

/// Every field type the generator understands. `php_type` and `field_js` fall
/// back to a string column / `field.<type>()` call for anything else, so an
/// unknown type would otherwise reach the generated PHP/TS and only fail at
/// lint or runtime. Reject it up front and list the supported set.
const VALID_FIELD_TYPES: [&str; 15] = [
    "text",
    "textarea",
    "email",
    "phone",
    "password",
    "number",
    "decimal",
    "boolean",
    "date",
    "time",
    "file",
    "select",
    "tags",
    "table",
    "foreign",
];

const RESERVED_COLUMN_NAMES: [&str; 15] = [
    "desc", "asc", "order", "group", "key", "index", "select", "insert", "update", "delete",
    "table", "from", "where", "by", "limit",
];

/// Every property `ResourceGrid` understands on a row action. Anything else is
/// silently dropped by the generated frontend, which hides the button without
/// any error, so unknown keys are rejected up front.
const ACTION_KEYS: [&str; 7] = [
    "permission",
    "actionExpression",
    "labelExpression",
    "label",
    "confirm",
    "handler",
    "form",
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
                        "items": { "type": "object", "additionalProperties": true }
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
                                    "description": "For `addField`: a full field definition object (name + type + properties). For `updateField`, `removeField`, and `moveField`: the existing field name as a string. To CHANGE an existing field, keep `field` as the current name and put every new property in `changes`."
                                },
                                "changes": {
                                    "type": "object",
                                    "additionalProperties": true,
                                    "description": "For `updateField` / `updateResource`: camelCase properties to set. A `null` value removes the property. To convert a field to a different type (e.g. number -> select), set `type` plus the matching properties here. This is how you change a field; `rename` only changes the name."
                                },
                                "rename": {
                                    "type": "string",
                                    "description": "For `updateField`: the NEW field name. Must differ from `field`; a `rename` equal to the current name is a no-op and is rejected. Renaming drops and re-adds the database column."
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
        let mut warnings: Vec<String> = Vec::new();

        for new_resource in &full_resources {
            let mut normalized_res = new_resource.clone();
            if let Some(fields) = normalized_res.get_mut("fields").and_then(Value::as_array_mut) {
                for f in fields {
                    if let Some(map) = f.as_object_mut() {
                        normalize_field_json(map);
                    }
                }
            }
            let name = resource_name(&normalized_res)
                .ok_or("Every resource in `resources` must have a non-empty `name`.")?
                .to_string();
            upsert_resource(&mut schema_resources, &name, normalized_res);
            push_unique(&mut affected, &name);
        }

        apply_operations(
            &mut schema_resources,
            &operations,
            &mut affected,
            &mut removed,
            &mut warnings,
        )?;

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
                validate_field_type(&resource, field)?;
                validate_foreign_conversion(&resource, field)?;
                // Validate computed expressions here, before any generated file
                // is written or a migration is run. Otherwise a bad expression
                // reaches `schemas.json` and only surfaces later from
                // `lint_code`, leaving the project in a broken state.
                validate_computed_field(&resource, field)?;
            }
            // Actions are validated against the RAW schema value: the typed
            // `ResourceAction` struct ignores unknown properties, so a stray
            // `label`/`title`/`onClick` would otherwise survive validation and
            // then be dropped by codegen, hiding the button with no error.
            validate_actions(&resource, schema_resources[index].get("actions"))?;
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
        for warning in &warnings {
            message.push_str(&format!(" Warning: {warning}"));
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
                "warnings": warnings,
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

/// Lists a resource's field names for error messages so the model can correct
/// a mistyped `field` without another `read_schema` round trip.
fn list_field_names(resource: &Value) -> String {
    resource
        .get("fields")
        .and_then(Value::as_array)
        .map(|fields| {
            fields
                .iter()
                .filter_map(field_name)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
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

/// Applies `operations` in order as one all-or-nothing unit.
///
/// `resources` is only mutated when every operation succeeds, so a bad
/// operation can never leave a half-applied schema behind. The returned error
/// states that nothing was applied and lists the resource's field names, which
/// is usually enough for the caller to correct the operation without a
/// `read_schema` round trip.
fn apply_operations(
    resources: &mut Vec<Value>,
    operations: &[Value],
    affected: &mut Vec<String>,
    removed: &mut Vec<String>,
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    let mut working = resources.clone();
    let mut working_affected = affected.clone();
    let mut working_removed = removed.clone();
    let mut working_warnings: Vec<String> = Vec::new();

    reject_oscillating_renames(operations)?;

    for (position, operation) in operations.iter().enumerate() {
        let resource = operation
            .get("resource")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let outcome = apply_operation_collecting(&mut working, operation, &mut working_warnings).map_err(|error| {
            // Operations run in order, so earlier ones have already been
            // applied to `working`. Nothing reached disk, but say so: a
            // partial edit silently discarded is what makes retries confusing.
            let discarded = match position {
                0 => String::new(),
                1 => " Operation #1 was valid but was discarded.".to_string(),
                n => format!(" Operations #1-#{n} were valid but were discarded."),
            };
            let available = find_resource_index(&working, &resource)
                .map(|index| list_field_names(&working[index]))
                .unwrap_or_default();
            let hint = if available.is_empty() {
                String::new()
            } else {
                format!(" Fields on '{resource}': {available}.")
            };
            format!(
                "Operation #{} failed: {error}{discarded} No changes were applied.{hint}",
                position + 1
            )
        })?;
        match outcome {
            OperationOutcome::Touched(name) => push_unique(&mut working_affected, &name),
            OperationOutcome::Removed(name) => {
                working_affected.retain(|existing| !existing.eq_ignore_ascii_case(&name));
                push_unique(&mut working_removed, &name);
            }
        }
    }

    *resources = working;
    *affected = working_affected;
    *removed = working_removed;
    warnings.extend(working_warnings);
    Ok(())
}

/// Rejects an operation list whose renames cancel each other out.
///
/// An agent stuck on a field tends to cycle `A -> B -> A` or `A -> tmp -> A`.
/// Every individual rename succeeds, so the run looks productive while it
/// drops and recreates the same database column repeatedly. Treat the whole
/// list as invalid and say why.
fn reject_oscillating_renames(operations: &[Value]) -> Result<(), String> {
    let mut renames: Vec<(String, String, String)> = Vec::new();
    for operation in operations {
        let Some(object) = operation.as_object() else {
            continue;
        };
        if object.get("action").and_then(Value::as_str) != Some("updateField") {
            continue;
        }
        let Some(resource) = object.get("resource").and_then(Value::as_str) else {
            continue;
        };
        let Some(field) = object
            .get("field")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
        else {
            continue;
        };
        let Some(rename) = object
            .get("rename")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty() && !name.eq_ignore_ascii_case(field))
        else {
            continue;
        };
        renames.push((resource.to_string(), field.to_string(), rename.to_string()));
    }

    // A list oscillates when the same field is renamed twice (`A -> B -> A`),
    // or when a name is both produced and consumed (`A -> B` then `B -> C`),
    // which means the intermediate step only exists to shuffle one column
    // around. Neither can be what the caller meant.
    for (index, (resource, from, to)) in renames.iter().enumerate() {
        for (other_resource, other_from, other_to) in renames.iter().skip(index + 1) {
            let same_resource = resource.eq_ignore_ascii_case(other_resource);
            if !same_resource {
                continue;
            }
            let renamed_twice = from.eq_ignore_ascii_case(other_from);
            let chained = to.eq_ignore_ascii_case(other_from);
            let restored = to.eq_ignore_ascii_case(other_to);
            if renamed_twice || chained || restored {
                return Err(format!(
                    "renaming field '{from}' to '{to}' (and again within the same call) has no effect \
                     except dropping and re-adding the database column. Decide on the final field name \
                     and send one `updateField`. To change the field's type or other properties, keep \
                     `field` as '{from}' and put them in `changes`."
                ));
            }
        }
    }
    Ok(())
}

/// Applies one incremental edit operation to the schema resource list.
#[cfg(test)]
fn apply_operation(
    resources: &mut Vec<Value>,
    operation: &Value,
) -> Result<OperationOutcome, String> {
    apply_operation_collecting(resources, operation, &mut Vec::new())
}

/// [`apply_operation`] variant that reports non-fatal side effects (e.g. a
/// rename dropping a column) through `warnings`.
fn apply_operation_collecting(
    resources: &mut Vec<Value>,
    operation: &Value,
    warnings: &mut Vec<String>,
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
            let mut field_map = field.as_object().unwrap().clone();
            normalize_field_json(&mut field_map);
            let name = field_name(&Value::Object(field_map.clone()))
                .ok_or("`addField` requires `field.name`")?
                .to_string();
            if field_map.get("type").and_then(Value::as_str).is_none() {
                return Err(format!("field '{name}' requires a `type`"));
            }
            if field_index(&resources[index], &name).is_some() {
                return Err(format!(
                    "field '{name}' already exists on resource '{resource_name}'; use `updateField` instead"
                ));
            }
            let fields = require_fields_mut(&mut resources[index])?;
            let position = resolve_insert_index(object, fields)?;
            fields.insert(position, Value::Object(field_map));
        }
        "updateField" => {
            let name = if let Some(n) = object
                .get("field")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|n| !n.is_empty())
            {
                n.to_string()
            } else if let Some(field_obj) = object.get("field").and_then(Value::as_object) {
                field_obj
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|n| !n.is_empty())
                    .ok_or("`updateField` requires `field.name` when `field` is an object")?
                    .to_string()
            } else {
                return Err("`updateField` requires `field` to be the existing field name or an object with `name`".into());
            };

            let mut changes = object
                .get("changes")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();

            if let Some(field_obj) = object.get("field").and_then(Value::as_object) {
                for (k, v) in field_obj {
                    if k != "name" && !changes.contains_key(k) {
                        changes.insert(k.clone(), v.clone());
                    }
                }
            }

            if changes.is_empty() {
                const RESERVED_OP_KEYS: [&str; 8] = [
                    "action", "resource", "field", "changes", "rename", "before", "after", "index",
                ];
                for (k, v) in object {
                    if !RESERVED_OP_KEYS.contains(&k.as_str()) {
                        changes.insert(k.clone(), v.clone());
                    }
                }
            }

            normalize_field_json(&mut changes);

            let requested_rename = object
                .get("rename")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let rename = requested_rename.filter(|value| !value.eq_ignore_ascii_case(&name));

            // A `rename` that repeats the current name is the single most common
            // dead end: the model intends to "re-declare" the field with new
            // properties but puts them nowhere, then retries the identical call.
            // Say what went wrong, show the corrected shape for this exact
            // resource/field, and echo what was received.
            if changes.is_empty()
                && rename.is_none()
                && requested_rename.is_some_and(|value| value.eq_ignore_ascii_case(&name))
            {
                let current = field_index(&resources[index], &name)
                    .map(|position| resources[index]["fields"][position].clone())
                    .unwrap_or(Value::Null);
                let suggestion = suggest_field_change(&current);
                return Err(format!(
                    "`updateField` on field '{name}' of resource '{resource_name}': `rename` is set to '{name}', \
                     which is the field's current name, so the operation is a no-op. `rename` only changes the \
                     name. To change properties, keep `field` as '{name}' and put them in `changes`, for example: \
                     {{ \"action\": \"updateField\", \"resource\": \"{resource_name}\", \"field\": \"{name}\", \
                     \"changes\": {suggestion} }}. To change the field's type, put the new type in `changes` too \
                     (e.g. \"changes\": {{ \"type\": \"number\" }}).\n\
                     Received operation: {}\nCurrent field definition: {}",
                    serde_json::to_string(operation).unwrap_or_default(),
                    serde_json::to_string(&current).unwrap_or_default()
                ));
            }

            if changes.is_empty() && rename.is_none() {
                return Err(format!(
                    "`updateField` on field '{name}' of resource '{resource_name}' requires at least one property \
                     change in `changes` or a new name in `rename`. Put the properties to change in `changes`, for \
                     example: {{ \"action\": \"updateField\", \"resource\": \"{resource_name}\", \"field\": \"{name}\", \
                     \"changes\": {{ \"required\": true, \"table\": true }} }}.\n\
                     Received operation: {}",
                    serde_json::to_string(operation).unwrap_or_default()
                ));
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
            normalize_field_json(field);
            if rename.is_some() {
                field.insert("name".to_string(), Value::String(new_name.clone()));
                warnings.push(format!(
                    "Renamed field '{name}' to '{new_name}' on resource '{resource_name}': the old column \
                     was dropped and a new one created, so existing values in it were not migrated."
                ));
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

fn collapse_backslashes(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut in_backslash = false;
    for ch in s.chars() {
        if ch == '\\' {
            if !in_backslash {
                result.push('\\');
                in_backslash = true;
            }
        } else {
            in_backslash = false;
            result.push(ch);
        }
    }
    result
}

fn split_function_args(s: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut depth = 0;
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut prev_char = '\0';

    for ch in s.chars() {
        if in_single_quote {
            current.push(ch);
            if ch == '\'' && prev_char != '\\' {
                in_single_quote = false;
            }
        } else if in_double_quote {
            current.push(ch);
            if ch == '"' && prev_char != '\\' {
                in_double_quote = false;
            }
        } else {
            match ch {
                '\'' => {
                    in_single_quote = true;
                    current.push(ch);
                }
                '"' => {
                    in_double_quote = true;
                    current.push(ch);
                }
                '(' => {
                    depth += 1;
                    current.push(ch);
                }
                ')' => {
                    if depth > 0 {
                        depth -= 1;
                    }
                    current.push(ch);
                }
                ',' if depth == 0 => {
                    args.push(current.trim().to_string());
                    current.clear();
                }
                _ => {
                    current.push(ch);
                }
            }
        }
        prev_char = ch;
    }
    if !current.trim().is_empty() {
        args.push(current.trim().to_string());
    }
    args
}

fn find_matching_paren(s: &str, start_after_open: usize) -> Option<usize> {
    let mut depth = 1;
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut prev_char = '\0';

    for (idx, ch) in s[start_after_open..].char_indices() {
        let abs_idx = start_after_open + idx;
        if in_single_quote {
            if ch == '\'' && prev_char != '\\' {
                in_single_quote = false;
            }
        } else if in_double_quote {
            if ch == '"' && prev_char != '\\' {
                in_double_quote = false;
            }
        } else {
            match ch {
                '\'' => in_single_quote = true,
                '"' => in_double_quote = true,
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(abs_idx);
                    }
                }
                _ => {}
            }
        }
        prev_char = ch;
    }
    None
}

fn rewrite_sql_functions(mut expr: String) -> String {
    let targets = ["GREATEST", "LEAST", "IFNULL", "NVL", "IF"];
    let mut changed = true;
    let mut iterations = 0;

    while changed && iterations < 50 {
        changed = false;
        iterations += 1;

        let upper = expr.to_ascii_uppercase();
        let mut earliest_match: Option<(usize, usize, &'static str)> = None;

        for &target in &targets {
            let mut search_from = 0;
            while let Some(pos) = upper[search_from..].find(target) {
                let abs_pos = search_from + pos;
                search_from = abs_pos + target.len();

                if abs_pos > 0 {
                    let prev_char = expr[..abs_pos].chars().last().unwrap();
                    if prev_char.is_alphanumeric() || prev_char == '_' || prev_char == '$' {
                        continue;
                    }
                }

                let after = &expr[abs_pos + target.len()..];
                let trimmed_after = after.trim_start();
                if trimmed_after.starts_with('(') {
                    let open_paren = abs_pos + target.len() + (after.len() - trimmed_after.len());
                    if earliest_match.map_or(true, |(earliest, _, _)| abs_pos < earliest) {
                        earliest_match = Some((abs_pos, open_paren, target));
                    }
                    break;
                }
            }
        }

        if let Some((start_pos, open_paren, target)) = earliest_match {
            if let Some(close_paren) = find_matching_paren(&expr, open_paren + 1) {
                let args_str = &expr[open_paren + 1..close_paren];
                let args = split_function_args(args_str);

                let replacement = match target {
                    "IFNULL" | "NVL" if args.len() == 2 => {
                        format!("COALESCE({}, {})", args[0], args[1])
                    }
                    "IF" if args.len() == 3 => {
                        format!("(CASE WHEN {} THEN ({}) ELSE ({}) END)", args[0], args[1], args[2])
                    }
                    "GREATEST" if args.len() >= 2 => {
                        let mut res = format!("(CASE WHEN ({}) >= ({}) THEN ({}) ELSE ({}) END)", args[0], args[1], args[0], args[1]);
                        for next_arg in &args[2..] {
                            res = format!("(CASE WHEN ({}) >= ({}) THEN ({}) ELSE ({}) END)", res, next_arg, res, next_arg);
                        }
                        res
                    }
                    "LEAST" if args.len() >= 2 => {
                        let mut res = format!("(CASE WHEN ({}) <= ({}) THEN ({}) ELSE ({}) END)", args[0], args[1], args[0], args[1]);
                        for next_arg in &args[2..] {
                            res = format!("(CASE WHEN ({}) <= ({}) THEN ({}) ELSE ({}) END)", res, next_arg, res, next_arg);
                        }
                        res
                    }
                    _ => {
                        continue;
                    }
                };

                expr.replace_range(start_pos..=close_paren, &replacement);
                changed = true;
            }
        }
    }

    expr
}

fn qualify_bare_entity_names(expr: &str) -> String {
    let mut result = String::with_capacity(expr.len() + 32);
    let mut chars = expr.char_indices().peekable();

    while let Some((i, ch)) = chars.next() {
        result.push(ch);
        let is_from = if (ch == 'F' || ch == 'f') && i + 4 <= expr.len() {
            expr[i..].to_ascii_uppercase().starts_with("FROM")
                && (i == 0 || !expr[..i].chars().last().unwrap().is_alphanumeric())
        } else {
            false
        };
        let is_join = if (ch == 'J' || ch == 'j') && i + 4 <= expr.len() {
            expr[i..].to_ascii_uppercase().starts_with("JOIN")
                && (i == 0 || !expr[..i].chars().last().unwrap().is_alphanumeric())
        } else {
            false
        };

        if is_from || is_join {
            let kw_len = 4;
            for _ in 0..kw_len - 1 {
                if let Some((_, c)) = chars.next() {
                    result.push(c);
                }
            }

            let mut ws = String::new();
            while let Some(&(_, c)) = chars.peek() {
                if c.is_whitespace() {
                    ws.push(c);
                    chars.next();
                } else {
                    break;
                }
            }

            if ws.is_empty() {
                continue;
            }
            result.push_str(&ws);

            let mut ident = String::new();
            while let Some(&(_, c)) = chars.peek() {
                if c.is_alphanumeric() || c == '_' || c == '\\' || c == '.' {
                    ident.push(c);
                    chars.next();
                } else {
                    break;
                }
            }

            if !ident.is_empty() {
                let first = ident.chars().next().unwrap();
                let upper_ident = ident.to_ascii_uppercase();
                let is_keyword = matches!(upper_ident.as_str(), "SELECT" | "INDEX" | "SET");
                if first.is_ascii_uppercase() && !ident.contains('\\') && !ident.contains('.') && !is_keyword {
                    result.push_str("App\\Entity\\");
                }
                result.push_str(&ident);
            }
        }
    }

    result
}

pub fn normalize_sql_expression(expr: &str) -> String {
    let collapsed = collapse_backslashes(expr);
    let qualified = qualify_bare_entity_names(&collapsed);
    rewrite_sql_functions(qualified)
}

fn normalize_field_json(field: &mut Map<String, Value>) {
    if let Some(val) = field.get("sqlExpression").and_then(Value::as_str) {
        field.insert("sqlExpression".to_string(), Value::String(normalize_sql_expression(val)));
    }
    if let Some(val) = field.get("sql_expression").and_then(Value::as_str) {
        field.insert("sql_expression".to_string(), Value::String(normalize_sql_expression(val)));
    }
}

const DQL_SHAPES_HINT: &str = r#"Doctrine DQL only allows a subquery as the ENTIRE select expression. A subquery cannot
be an operand of +, -, *, or / - the parser reports `Expected TokenType::T_FROM, got '-'`.
Use one of these shapes instead:
  1. a single subquery:
       (SELECT COALESCE(SUM(i.amount), 0) FROM App\Entity\Item i WHERE i.parentId = e.id)
  2. arithmetic over columns already stored on the entity:
       {EXAMPLE}
  3. a subquery wrapped in COALESCE:
       COALESCE((SELECT SUM(i.amount) FROM App\Entity\Item i WHERE i.parentId = e.id), 0)"#;

/// Finds a parenthesised `SELECT ...` used as an operand of `+ - * /`.
///
/// Doctrine DQL accepts a subselect only as the *entire* select expression: a
/// leading `(SELECT ...)` makes the parser consume the subquery and then expect
/// the outer `FROM`. String literals are skipped so quoted text is never
/// mistaken for a subquery.
fn subquery_operand_violation(expr: &str) -> Option<String> {
    let is_arithmetic = |ch: char| matches!(ch, '+' | '-' | '*' | '/');
    let mut in_single = false;
    let mut in_double = false;
    let mut ranges: Vec<(usize, usize)> = Vec::new();

    for (index, ch) in expr.char_indices() {
        match ch {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '(' if !in_single && !in_double => {
                let rest = expr[index + 1..].trim_start();
                let is_select = rest.len() >= 6
                    && rest[..6].eq_ignore_ascii_case("SELECT")
                    && rest
                        .as_bytes()
                        .get(6)
                        .map_or(true, |byte| !byte.is_ascii_alphanumeric() && *byte != b'_');
                if is_select {
                    if let Some(close) = find_matching_paren(expr, index + 1) {
                        ranges.push((index, close));
                    }
                }
            }
            _ => {}
        }
    }

    for (open, close) in ranges {
        let trailing = expr[close + 1..].trim_start().chars().next();
        let leading = expr[..open].trim_end().chars().next_back();
        if trailing.is_some_and(is_arithmetic) || leading.is_some_and(is_arithmetic) {
            return Some(expr[open..=close].to_string());
        }
    }

    None
}

/// Names of columns already stored on the entity, i.e. fields usable as
/// `e.<name>` from another field's `sqlExpression`.
///
/// Includes plain numeric/decimal columns (`receivedAmount`) as well as stored
/// computed ones (`totalAmount`), since arithmetic over either is legal DQL.
fn stored_column_names(resource: &Resource, exclude: &str) -> Vec<String> {
    resource
        .fields
        .iter()
        .filter(|field| {
            !field.name.eq_ignore_ascii_case(exclude)
                && field.field_type != "table"
                && matches!(
                    field.field_type.as_str(),
                    "decimal" | "number" | "integer" | "currency" | "float"
                )
        })
        .map(|field| field.name.clone())
        .collect()
}

/// Validates the row-action array of a resource.
///
/// `raw_actions` is the un-deserialised `actions` value from the schema, because
/// the typed `ResourceAction` silently ignores unknown properties.
///
/// Rejected cases (each one otherwise hides the button with no error):
/// - a property outside [`ACTION_KEYS`] (e.g. `title`, `icon`, `onClick`);
/// - an action with neither `label` nor `labelExpression`;
/// - an action with nothing to run (`handler`, `form`, or `actionExpression`);
/// - an action with both `handler` and `form`.
fn validate_actions(resource: &Resource, raw_actions: Option<&Value>) -> Result<(), String> {
    let actions = match raw_actions.and_then(Value::as_array) {
        Some(actions) if !actions.is_empty() => actions,
        _ => return Ok(()),
    };

    let allowed = ACTION_KEYS.join(", ");

    for (position, raw) in actions.iter().enumerate() {
        let number = position + 1;
        let Some(object) = raw.as_object() else {
            return Err(format!(
                "Action #{number} on resource '{}' must be an object, got: {}",
                resource.name,
                serde_json::to_string(raw).unwrap_or_default()
            ));
        };

        if let Some(unknown) = object.keys().find(|key| !ACTION_KEYS.contains(&key.as_str())) {
            let hint = match unknown.as_str() {
                "title" | "icon" | "onClick" | "on_click" => {
                    " Row actions are declarative: there is no `onClick`/`icon`/`title` support. Use `handler` for custom JavaScript and `label` for the button text."
                }
                _ => "",
            };
            return Err(format!(
                "Action #{number} on resource '{}' uses unsupported property `{unknown}`.{hint}\n\
                 Supported action properties: {allowed}.\n\
                 `label` is a static button text; `labelExpression` is a JavaScript expression (a string literal must be quoted inside it, e.g. labelExpression: \"'Download PDF'\").\n\
                 Received action: {}",
                resource.name,
                serde_json::to_string(raw).unwrap_or_default()
            ));
        }

        let has_label = object
            .get("label")
            .and_then(Value::as_str)
            .map(|label| !label.trim().is_empty())
            .unwrap_or(false);
        let has_label_expression = object
            .get("labelExpression")
            .and_then(Value::as_str)
            .map(|expression| !expression.trim().is_empty())
            .unwrap_or(false);
        if !has_label && !has_label_expression {
            return Err(format!(
                "Action #{number} on resource '{}' has neither `label` nor `labelExpression`, so its button can never render.\n\
                 Use `label: \"Download PDF\"` for a constant label, or `labelExpression: \"data.status === 'draft' ? 'Issue' : null\"` to build it per row.\n\
                 Received action: {}",
                resource.name,
                serde_json::to_string(raw).unwrap_or_default()
            ));
        }

        let has_handler = object.contains_key("handler");
        let has_form = object.contains_key("form");
        let has_action_expression = object.contains_key("actionExpression");
        if has_handler && has_form {
            return Err(format!(
                "Action #{number} on resource '{}' sets both `handler` and `form`; only one of them may be used.\n\
                 Use `handler` for inline JavaScript, or `form` for a dialog that submits to an endpoint.\n\
                 Received action: {}",
                resource.name,
                serde_json::to_string(raw).unwrap_or_default()
            ));
        }
        if !has_handler && !has_form && !has_action_expression {
            return Err(format!(
                "Action #{number} on resource '{}' has nothing to run: provide `handler` (custom JavaScript), `form` (a dialog), or `actionExpression` (a path POSTed to the backend).\n\
                 Received action: {}",
                resource.name,
                serde_json::to_string(raw).unwrap_or_default()
            ));
        }
    }

    Ok(())
}

/// Picks a `changes` example that would actually change something on `field`.
///
/// A blanket `{ "required": true }` is useless when the field is already
/// required — the model reads it, sees nothing to do, and retries the same
/// no-op `rename`. Prefer a broken expression to replace, then a flag the field
/// does not already have switched on.
fn suggest_field_change(field: &Value) -> String {
    let Some(map) = field.as_object() else {
        return "{ \"label\": \"New Label\" }".to_string();
    };
    if map.contains_key("sqlExpression") {
        return "{ \"sqlExpression\": \"e.totalAmount - COALESCE(e.receivedAmount, 0)\" }".to_string();
    }
    if map.contains_key("computeExpression") {
        return "{ \"computeExpression\": \"data.a - data.b\" }".to_string();
    }
    // First display flag the field does not already have switched on.
    for (key, example) in [
        ("filterable", "{ \"filterable\": true }"),
        ("searchable", "{ \"searchable\": true }"),
        ("sortable", "{ \"sortable\": true }"),
    ] {
        if !matches!(map.get(key), Some(Value::Bool(true))) {
            return example.to_string();
        }
    }
    "{ \"label\": \"New Label\" }".to_string()
}

/// Rejects a field whose `type` is not one the generator understands.
fn validate_field_type(resource: &Resource, field: &Field) -> Result<(), String> {
    if VALID_FIELD_TYPES.contains(&field.field_type.as_str()) {
        return Ok(());
    }
    Err(format!(
        "Field '{}' on resource '{}' has unknown type '{}'. Supported types: {}.",
        field.name,
        resource.name,
        field.field_type,
        VALID_FIELD_TYPES.join(", ")
    ))
}

/// Rejects converting a reference field to the `foreign` type.
///
/// `select` + `resource` is the user-facing dropdown and intentionally stores a
/// `?string` column. `foreign` is the bare integer parent-reference used by
/// line items, and it renders as a plain number input (there is no `foreign`
/// case in `FormField.tsx`). Swapping one for the other silently degrades the
/// form and drops + recreates the column, so point at the supported option
/// instead of accepting the edit.
fn validate_foreign_conversion(resource: &Resource, field: &Field) -> Result<(), String> {
    if field.field_type != "foreign" {
        return Ok(());
    }
    let Some(reference) = field
        .resource
        .as_ref()
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };
    Err(format!(
        "Field '{}' on resource '{}' cannot be `type: \"foreign\"` because it also has a `resource` \
         ({reference}). Use `type: \"select\"` with `resource` for a dropdown bound to another \
         resource — its `?string` column is intentional, so cast with `(int)` in backend code when \
         you need the referenced id. `type: \"foreign\"` is only for a child line's integer parent \
         reference and renders as a plain number input. Never hand-edit the generated entity to \
         change the column type; it is regenerated on every sync.",
        field.name, resource.name
    ))
}

/// Rejects computed definitions that cannot work: an `sqlExpression` without
/// `computed: true`, a computed field with nothing to compute from, and a
/// subquery used as an arithmetic operand.
fn validate_computed_field(resource: &Resource, field: &Field) -> Result<(), String> {
    if !field.computed.unwrap_or(false) {
        if field.sql_expression.is_some() {
            return Err(format!(
                "Field '{}' on resource '{}' sets `sqlExpression` but is not marked `computed: true`, so the expression would be ignored. Set `computed: true` as well, or remove `sqlExpression`.",
                field.name, resource.name
            ));
        }
        return Ok(());
    }

    if field.sql_expression.is_none() && field.compute_expression.is_none() {
        return Err(format!(
            "Computed field '{}' on resource '{}' has neither `sqlExpression` nor `computeExpression`, so it can never be populated. Add a `sqlExpression` (server-side stored column) and/or a `computeExpression` (frontend preview).",
            field.name, resource.name
        ));
    }

    let Some(expr) = field.sql_expression.as_deref().map(str::trim) else {
        return Ok(());
    };
    if expr.is_empty() {
        return Ok(());
    }

    let Some(offending) = subquery_operand_violation(expr) else {
        return Ok(());
    };

    let stored = stored_column_names(resource, &field.name);
    let stored_hint = if stored.is_empty() {
        format!(
            "Resource '{}' has no other numeric columns yet. Add the child rollup as its own computed field first, then reference it here as `e.<name>`.",
            resource.name
        )
    } else {
        format!(
            "Numeric columns on '{}' you can reference directly as `e.<name>`: {}.",
            resource.name,
            stored.join(", ")
        )
    };
    let example = if stored.len() >= 2 {
        format!("e.{} - COALESCE(e.{}, 0)", stored[0], stored[1])
    } else if let Some(only) = stored.first() {
        format!("e.{} - COALESCE(e.<otherStoredColumn>, 0)", only)
    } else {
        "e.<storedColumn> - COALESCE(e.<otherStoredColumn>, 0)".to_string()
    };

    Err(format!(
        "Computed field '{}' on resource '{}' has an invalid `sqlExpression`:\n  {}\n\nOffending fragment: {}\n\n{}\n\n{}",
        field.name,
        resource.name,
        expr,
        offending,
        DQL_SHAPES_HINT.replace("{EXAMPLE}", &example),
        stored_hint
    ))
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
            if let Some(label) = &a.label {
                s.push_str(&format!("      label: {},\n", js(label)));
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
    let normalized_sql = f.sql_expression.as_deref().map(normalize_sql_expression);
    call!("computeExpression", f.compute_expression.as_deref());
    call!("sqlExpression", normalized_sql.as_deref());
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
                    let normalized = normalize_sql_expression(expr);
                    let escaped = normalized.replace('\\', "\\\\").replace('\'', "\\'");
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

    fn resource_with_actions(actions: Value) -> (Resource, Value) {
        let raw = json!({
            "name": "sales",
            "endpoint": "/sales",
            "fields": [{ "name": "invoiceNumber", "type": "text" }],
            "actions": actions
        });
        let resource: Resource = serde_json::from_value(raw.clone()).unwrap();
        let actions = raw["actions"].clone();
        (resource, actions)
    }

    #[test]
    fn accepts_static_and_expression_labels() {
        let (resource, actions) = resource_with_actions(json!([
            { "label": "Download PDF", "handler": "await api.openPdf('/pdf/invoice/' + data.id);" },
            { "labelExpression": "'Receive Payment'", "form": { "title": "Pay" } },
            { "labelExpression": "data.status !== 'paid' ? 'Pay' : null", "actionExpression": "/pay" }
        ]));
        assert!(validate_actions(&resource, Some(&actions)).is_ok());
    }

    #[test]
    fn accepts_action_without_actions_array() {
        let (resource, _) = resource_with_actions(Value::Null);
        assert!(validate_actions(&resource, None).is_ok());
        assert!(validate_actions(&resource, Some(&Value::Null)).is_ok());
    }

    #[test]
    fn rejects_unsupported_action_property() {
        let (resource, actions) = resource_with_actions(json!([
            { "label": "Invoice", "icon": "<FilePdfOutlined />", "onClick": "open(id)" }
        ]));
        let error = validate_actions(&resource, Some(&actions)).unwrap_err();
        assert!(error.contains("unsupported property `icon`"), "{error}");
        assert!(error.contains("Supported action properties"), "{error}");
    }

    #[test]
    fn rejects_action_without_label() {
        let (resource, actions) = resource_with_actions(json!([
            { "handler": "doThing();" }
        ]));
        let error = validate_actions(&resource, Some(&actions)).unwrap_err();
        assert!(error.contains("neither `label` nor `labelExpression`"), "{error}");
    }

    #[test]
    fn rejects_blank_label() {
        let (resource, actions) = resource_with_actions(json!([
            { "label": "   ", "handler": "doThing();" }
        ]));
        assert!(validate_actions(&resource, Some(&actions)).is_err());
    }

    #[test]
    fn rejects_action_with_nothing_to_run() {
        let (resource, actions) = resource_with_actions(json!([
            { "label": "Download PDF" }
        ]));
        let error = validate_actions(&resource, Some(&actions)).unwrap_err();
        assert!(error.contains("nothing to run"), "{error}");
    }

    #[test]
    fn rejects_handler_and_form_together() {
        let (resource, actions) = resource_with_actions(json!([
            { "label": "Both", "handler": "x();", "form": { "title": "T" } }
        ]));
        let error = validate_actions(&resource, Some(&actions)).unwrap_err();
        assert!(error.contains("both `handler` and `form`"), "{error}");
    }

    #[test]
    fn actions_js_emits_static_label() {
        let (resource, _) = resource_with_actions(json!([
            { "label": "Download PDF", "handler": "await api.openPdf('/pdf/invoice/' + data.id);" }
        ]));
        let emitted = actions_js(&resource);
        assert!(emitted.contains("label: \"Download PDF\""), "{emitted}");
        assert!(!emitted.contains("labelExpression"), "{emitted}");
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

    #[test]
    fn normalizes_over_escaped_backslashes_in_sql_expression() {
        let double_escaped = "(SELECT COALESCE(SUM(i.quantity * i.unitPrice), 0) FROM App\\\\Entity\\\\SalesItem i WHERE i.saleId = e.id)";
        assert_eq!(
            normalize_sql_expression(double_escaped),
            "(SELECT COALESCE(SUM(i.quantity * i.unitPrice), 0) FROM App\\Entity\\SalesItem i WHERE i.saleId = e.id)"
        );

        let resource: Resource = serde_json::from_value(json!({
            "name": "sales",
            "endpoint": "/sales",
            "fields": [
                {
                    "name": "totalAmount",
                    "type": "decimal",
                    "scale": 2,
                    "computed": true,
                    "sqlExpression": "(SELECT COALESCE(SUM(i.quantity * i.unitPrice), 0) FROM App\\\\Entity\\\\SalesItem i WHERE i.saleId = e.id)"
                }
            ]
        }))
        .unwrap();

        let code = backend_code(&resource, "Sale");
        assert!(code.contains("#[Computed(sqlExpression: '(SELECT COALESCE(SUM(i.quantity * i.unitPrice), 0) FROM App\\\\Entity\\\\SalesItem i WHERE i.saleId = e.id)')]"));
    }

    #[test]
    fn qualifies_bare_entity_names_in_sql_expression() {
        let bare = "(SELECT COALESCE(SUM(i.quantity * i.unitPrice), 0) FROM SalesItem i WHERE i.saleId = e.id)";
        assert_eq!(
            normalize_sql_expression(bare),
            "(SELECT COALESCE(SUM(i.quantity * i.unitPrice), 0) FROM App\\Entity\\SalesItem i WHERE i.saleId = e.id)"
        );
    }

    #[test]
    fn rewrites_mysql_functions_in_sql_expression_to_dql() {
        let greatest_expr = "GREATEST(e.totalAmount - COALESCE(e.receivedAmount, 0), 0)";
        assert_eq!(
            normalize_sql_expression(greatest_expr),
            "(CASE WHEN (e.totalAmount - COALESCE(e.receivedAmount, 0)) >= (0) THEN (e.totalAmount - COALESCE(e.receivedAmount, 0)) ELSE (0) END)"
        );

        let ifnull_expr = "IFNULL(e.receivedAmount, 0)";
        assert_eq!(
            normalize_sql_expression(ifnull_expr),
            "COALESCE(e.receivedAmount, 0)"
        );

        let if_expr = "IF(e.totalAmount > 0, e.totalAmount, 0)";
        assert_eq!(
            normalize_sql_expression(if_expr),
            "(CASE WHEN e.totalAmount > 0 THEN (e.totalAmount) ELSE (0) END)"
        );
    }

    fn computed_resource(balance_expression: &str) -> Resource {
        serde_json::from_value(json!({
            "name": "sales",
            "endpoint": "/sales",
            "fields": [
                {
                    "name": "totalAmount",
                    "type": "decimal",
                    "computed": true,
                    "sqlExpression": "(SELECT COALESCE(SUM(i.quantity * i.unitPrice), 0) FROM App\\Entity\\SaleItem i WHERE i.saleId = e.id)"
                },
                { "name": "receivedAmount", "type": "decimal" },
                {
                    "name": "balanceAmount",
                    "type": "decimal",
                    "computed": true,
                    "sqlExpression": balance_expression
                }
            ]
        }))
        .unwrap()
    }

    #[test]
    fn accepts_legal_computed_expressions() {
        let legal = [
            // Pure subquery is the whole expression.
            "(SELECT COALESCE(SUM(i.quantity * i.unitPrice), 0) FROM App\\Entity\\SaleItem i WHERE i.saleId = e.id)",
            // Arithmetic over stored columns.
            "e.totalAmount - COALESCE(e.receivedAmount, 0)",
            // CASE over stored columns.
            "CASE WHEN e.totalAmount > 0 THEN e.totalAmount ELSE 0 END",
            // Subquery wrapped in COALESCE.
            "COALESCE((SELECT SUM(i.unitPrice) FROM App\\Entity\\SaleItem i WHERE i.saleId = e.id), 0)",
        ];
        for expression in legal {
            let resource = computed_resource(expression);
            let field = resource
                .fields
                .iter()
                .find(|field| field.name == "balanceAmount")
                .unwrap();
            assert!(
                validate_computed_field(&resource, field).is_ok(),
                "expected {expression} to be accepted"
            );
        }
    }

    /// The exact expression from the production incident: a child rollup
    /// subquery subtracted from a stored column. Doctrine parses the subquery as
    /// the whole select expression and then expects the outer FROM.
    #[test]
    fn rejects_subquery_used_as_arithmetic_operand() {
        let expression = "(SELECT COALESCE(SUM(i.quantity * i.unitPrice), 0) FROM App\\Entity\\SaleItem i WHERE i.saleId = e.id) - COALESCE(e.receivedAmount, 0)";
        let resource = computed_resource(expression);
        let field = resource
            .fields
            .iter()
            .find(|field| field.name == "balanceAmount")
            .unwrap();

        let error = validate_computed_field(&resource, field).unwrap_err();
        assert!(error.contains("balanceAmount"), "{error}");
        assert!(error.contains("T_FROM"), "{error}");
        // Self-correcting: names the stored columns it can reference instead.
        assert!(error.contains("totalAmount"), "{error}");
        assert!(error.contains("e.totalAmount - COALESCE(e.receivedAmount, 0)"), "{error}");
    }

    #[test]
    fn rejects_two_subqueries_in_arithmetic() {
        let expression = "((SELECT COALESCE(SUM(i.quantity * i.unitPrice), 0) FROM App\\Entity\\SaleItem i WHERE i.saleId = e.id) - (SELECT COALESCE(SUM(p.amount), 0) FROM App\\Entity\\Payment p WHERE p.saleId = e.id))";
        assert!(subquery_operand_violation(expression).is_some());
    }

    #[test]
    fn ignores_arithmetic_inside_string_literals() {
        assert!(subquery_operand_violation("'a - b'").is_none());
        assert!(subquery_operand_violation("CASE WHEN e.a > 0 THEN 'x - y' ELSE 'z' END").is_none());
    }

    #[test]
    fn rejects_sql_expression_without_computed_flag() {
        let resource: Resource = serde_json::from_value(json!({
            "name": "sales",
            "endpoint": "/sales",
            "fields": [
                { "name": "totalAmount", "type": "decimal", "sqlExpression": "e.a + e.b" }
            ]
        }))
        .unwrap();
        let error = validate_computed_field(&resource, &resource.fields[0]).unwrap_err();
        assert!(error.contains("computed: true"), "{error}");
    }

    #[test]
    fn rejects_computed_field_without_any_expression() {
        let resource: Resource = serde_json::from_value(json!({
            "name": "sales",
            "endpoint": "/sales",
            "fields": [
                { "name": "totalAmount", "type": "decimal", "computed": true }
            ]
        }))
        .unwrap();
        let error = validate_computed_field(&resource, &resource.fields[0]).unwrap_err();
        assert!(error.contains("computeExpression"), "{error}");
    }

    #[test]
    fn noop_rename_hint_suggests_the_mutable_property() {
        let mut resources = vec![json!({
            "name": "sales",
            "endpoint": "/sales",
            "fields": [
                {
                    "name": "balanceAmount",
                    "type": "decimal",
                    "computed": true,
                    "sqlExpression": "e.a - e.b"
                }
            ]
        })];

        let error = apply_operation(
            &mut resources,
            &json!({
                "action": "updateField",
                "resource": "sales",
                "field": "balanceAmount",
                "rename": "balanceAmount"
            }),
        )
        .unwrap_err();

        assert!(error.contains("sqlExpression"), "{error}");
        assert!(!error.contains("\"required\": true"), "{error}");
    }

    #[test]
    fn rejects_noop_update_field_with_same_rename_and_accepts_inline_changes() {
        let mut resources = schema_fixture();
        let error = apply_operation(
            &mut resources,
            &json!({
                "action": "updateField",
                "resource": "products",
                "field": "price",
                "rename": "price"
            }),
        )
        .unwrap_err();
        // A same-name rename gets the dedicated, self-correcting message.
        assert!(error.contains("no-op"), "{error}");
        assert!(error.contains("changes"), "{error}");
        // The suggestion must be a change the field does not already have: the
        // old `{"required": true}` example was a no-op for this field and the
        // model ignored it, then kept renaming.
        assert!(!error.contains("\"required\": true"), "{error}");
        assert!(error.contains("To change the field's type"), "{error}");
        assert!(error.contains("Received operation"), "{error}");
        assert!(error.contains("Current field definition"), "{error}");

        // Updating with field object
        apply_operation(
            &mut resources,
            &json!({
                "action": "updateField",
                "resource": "products",
                "field": {
                    "name": "price",
                    "sqlExpression": "GREATEST(e.price, 0)"
                }
            }),
        )
        .unwrap();
        let price_field = &resources[0]["fields"][1];
        assert_eq!(
            price_field["sqlExpression"],
            json!("(CASE WHEN (e.price) >= (0) THEN (e.price) ELSE (0) END)")
        );
    }

    /// A rename equal to the current name is the exact call the agent retried
    /// three times in production. The error must name the mistake, echo the
    /// received operation, and report the current field definition.
    #[test]
    fn noop_rename_error_reports_received_operation_and_current_field() {
        let mut resources = schema_fixture();
        let error = apply_operation(
            &mut resources,
            &json!({
                "action": "updateField",
                "resource": "products",
                "field": "price",
                "rename": "PRICE"
            }),
        )
        .unwrap_err();

        assert!(error.contains("no-op"), "{error}");
        assert!(error.contains("\"rename\""), "{error}");
        assert!(error.contains("Current field definition"), "{error}");
        assert!(error.contains("\"decimal\""), "{error}");
        // Rename matching is case-insensitive, so the error explains itself.
        assert!(error.contains("updateField"), "{error}");
    }

    /// Properties supplied at the top level (not in `changes`) still work, and
    /// that fallback must not be mistaken for a no-op.
    #[test]
    fn inline_top_level_properties_change_the_field() {
        let mut resources = schema_fixture();
        apply_operation(
            &mut resources,
            &json!({
                "action": "updateField",
                "resource": "products",
                "field": "price",
                "required": true
            }),
        )
        .unwrap();
        assert_eq!(resources[0]["fields"][1]["required"], json!(true));
    }

    /// Converts a scalar FK into a resource-bound select — the edit that failed
    /// in production because it was expressed as a no-op `rename`.
    #[test]
    fn converts_scalar_field_into_resource_select_via_changes() {
        let mut resources = schema_fixture();
        resources[0]["fields"]
            .as_array_mut()
            .unwrap()
            .push(json!({ "name": "saleId", "type": "number", "label": "Sale" }));

        apply_operation(
            &mut resources,
            &json!({
                "action": "updateField",
                "resource": "products",
                "field": "saleId",
                "changes": {
                    "type": "select",
                    "resource": "sales",
                    "required": true,
                    "form": true,
                    "table": true
                }
            }),
        )
        .unwrap();

        let sale_id = &resources[0]["fields"][2];
        assert_eq!(sale_id["type"], json!("select"));
        assert_eq!(sale_id["resource"], json!("sales"));
        assert_eq!(sale_id["required"], json!(true));
    }

    /// A failing operation must roll back the ones before it, and the error must
    /// say so. In production a valid `resources` payload plus one bad operation
    /// was discarded with only "Operation #1 failed: ...".
    #[test]
    fn failed_operation_rolls_back_earlier_operations() {
        let mut resources = schema_fixture();
        let original = resources.clone();
        let mut affected = Vec::new();
        let mut removed = Vec::new();

        let error = apply_operations(
            &mut resources,
            &[
                json!({
                    "action": "addField",
                    "resource": "products",
                    "field": { "name": "sku", "type": "text" }
                }),
                json!({
                    "action": "updateField",
                    "resource": "products",
                    "field": "price",
                    "rename": "price"
                }),
            ],
            &mut affected,
            &mut removed,
            &mut Vec::new(),
        )
        .unwrap_err();

        assert_eq!(resources, original, "schema must be untouched");
        assert!(affected.is_empty(), "no resource may be reported as touched");
        assert!(error.contains("Operation #2 failed"), "{error}");
        assert!(error.contains("Operation #1 was valid but was discarded"), "{error}");
        assert!(error.contains("No changes were applied"), "{error}");
        assert!(error.contains("Fields on 'products'"), "{error}");
        assert!(error.contains("sku") || error.contains("price"), "{error}");
    }

    /// Successful operations still commit and report the touched resource.
    #[test]
    fn successful_operations_commit_and_report_touched_resource() {
        let mut resources = schema_fixture();
        let mut affected = Vec::new();
        let mut removed = Vec::new();

        apply_operations(
            &mut resources,
            &[json!({
                "action": "addField",
                "resource": "products",
                "field": { "name": "sku", "type": "text" }
            })],
            &mut affected,
            &mut removed,
            &mut Vec::new(),
        )
        .unwrap();

        assert_eq!(field_names(&resources), ["name", "price", "sku"]);
        assert_eq!(affected, vec!["products".to_string()]);
        assert!(removed.is_empty());
    }

    fn select_field_schema() -> Vec<Value> {
        vec![json!({
            "name": "payments",
            "endpoint": "/payments",
            "fields": [
                {
                    "name": "saleId",
                    "type": "select",
                    "label": "Sale",
                    "resource": "sales",
                    "required": true
                }
            ]
        })]
    }

    #[test]
    fn rejects_unknown_field_type() {
        let resource: Resource = serde_json::from_value(json!({
            "name": "products",
            "endpoint": "/products",
            "fields": [{ "name": "price", "type": "currency" }]
        }))
        .unwrap();
        let error = validate_field_type(&resource, &resource.fields[0]).unwrap_err();
        assert!(error.contains("unknown type 'currency'"), "{error}");
        assert!(error.contains("Supported types"), "{error}");
        assert!(error.contains("decimal"), "{error}");
    }

    #[test]
    fn rejects_select_to_foreign_conversion() {
        let resource: Resource = serde_json::from_value(json!({
            "name": "payments",
            "endpoint": "/payments",
            "fields": [{
                "name": "saleId",
                "type": "foreign",
                "resource": "sales"
            }]
        }))
        .unwrap();
        let error = validate_foreign_conversion(&resource, &resource.fields[0]).unwrap_err();
        assert!(error.contains("cannot be `type: \"foreign\"`"), "{error}");
        assert!(error.contains("Use `type: \"select\"`"), "{error}");
        assert!(error.contains("plain number input"), "{error}");
    }

    #[test]
    fn allows_foreign_field_without_resource() {
        let mut resources = schema_fixture();
        apply_operation(
            &mut resources,
            &json!({
                "action": "addField",
                "resource": "products",
                "field": { "name": "categoryId", "type": "foreign" }
            }),
        )
        .unwrap();
        assert_eq!(field_names(&resources), ["name", "price", "categoryId"]);
    }

    #[test]
    fn no_op_rename_suggestion_does_not_repeat_existing_value() {
        let mut resources = select_field_schema();
        let error = apply_operation(
            &mut resources,
            &json!({
                "action": "updateField",
                "resource": "payments",
                "field": "saleId",
                "rename": "saleId"
            }),
        )
        .unwrap_err();
        // The incident's suggestion was `{"required": true}`, which the field
        // already had, so the model discarded it and kept renaming.
        assert!(!error.contains("\"required\": true"), "{error}");
        assert!(error.contains("To change the field's type"), "{error}");
        assert!(error.contains("no-op"), "{error}");
    }

    #[test]
    fn rename_reports_dropped_column_warning() {
        let mut resources = schema_fixture();
        let mut warnings = Vec::new();
        apply_operation_collecting(
            &mut resources,
            &json!({
                "action": "updateField",
                "resource": "products",
                "field": "price",
                "rename": "unitPrice"
            }),
            &mut warnings,
        )
        .unwrap();
        assert_eq!(field_names(&resources), ["name", "unitPrice"]);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("not migrated"), "{warnings:?}");
    }

    #[test]
    fn rejects_rename_round_trip_in_one_call() {
        let mut resources = schema_fixture();
        let error = apply_operations(
            &mut resources,
            &[
                json!({
                    "action": "updateField",
                    "resource": "products",
                    "field": "price",
                    "rename": "sale_id_tmp"
                }),
                json!({
                    "action": "updateField",
                    "resource": "products",
                    "field": "sale_id_tmp",
                    "rename": "price"
                }),
            ],
            &mut Vec::new(),
            &mut Vec::new(),
            &mut Vec::new(),
        )
        .unwrap_err();
        assert!(error.contains("has no effect"), "{error}");
        assert!(error.contains("dropping and re-adding"), "{error}");
    }

    #[test]
    fn allows_unrelated_renames_in_one_call() {
        let mut resources = schema_fixture();
        apply_operations(
            &mut resources,
            &[
                json!({
                    "action": "updateField",
                    "resource": "products",
                    "field": "name",
                    "rename": "title"
                }),
                json!({
                    "action": "updateField",
                    "resource": "products",
                    "field": "price",
                    "rename": "unitPrice"
                }),
            ],
            &mut Vec::new(),
            &mut Vec::new(),
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(field_names(&resources), ["title", "unitPrice"]);
    }
}
