use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::path::PathBuf;
use std::process::Command;

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
    length: Option<u64>,
    required: Option<bool>,
    unique: Option<bool>,
    email: Option<bool>,
    source: Option<String>,
    sortable: Option<bool>,
    searchable: Option<bool>,
    password: Option<bool>,
    accept: Option<String>,
    max_size: Option<u64>,
    resource: Option<Value>,
    map: Option<String>,
    columns: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Resource {
    name: String,
    endpoint: String,
    fields: Vec<Field>,
    title_expression: Option<String>,
}

pub struct SyncSchemaTool {
    parameters: Map<String, Value>,
}
impl SyncSchemaTool {
    pub fn new() -> Self {
        Self { parameters: json!({"type":"object","properties":{"resources":{"type":"array","description":"Structured resource definitions.","items":{"type":"object"}},"projectPath":{"type":"string"}},"required":["resources"]}).as_object().unwrap().clone() }
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
        "Generates React frontend resources and Symfony backend PHP entities from structured schema definitions, then runs migrations."
    }
    fn parameters(&self) -> &Map<String, Value> {
        &self.parameters
    }
    async fn execute(&self, args: Value) -> LlmResult<Value> {
        let resources: Vec<Resource> = serde_json::from_value(
            args.get("resources")
                .cloned()
                .ok_or("resources is required")?,
        )?;
        if resources.is_empty() {
            return Err("resources must contain at least one resource".into());
        }
        let project = PathBuf::from(
            args.get("projectPath")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .unwrap_or("content/dummy-project"),
        );
        let mut frontend = Vec::new();
        let mut backend = Vec::new();
        for resource in &resources {
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
            let singular = singular(&resource.name);
            let class = pascal(&singular);
            let dir = project.join("backend/src/Entity");
            std::fs::create_dir_all(&dir)?;
            let bp = dir.join(format!("{}.php", class));
            std::fs::write(&bp, backend_code(resource, &class))?;
            backend.push(bp);
        }
        let backend_dir = project.join("backend");
        for command in [
            vec!["bin/console", "make:migration"],
            vec![
                "bin/console",
                "doctrine:migrations:migrate",
                "--no-interaction",
            ],
        ] {
            let status = Command::new("php")
                .args(&command)
                .current_dir(&backend_dir)
                .status()?;
            if !status.success() {
                return Err(format!("Migration command failed: php {}", command.join(" ")).into());
            }
        }
        Ok(
            json!({"success":true,"message":"Schema sync completed successfully.","details":{"resources":resources.iter().map(|r|r.name.clone()).collect::<Vec<_>>(),"frontendResourcePaths":frontend,"backendEntityPaths":backend,"timestamp":timestamp()}}),
        )
    }
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
    format!("import defineResource from '../utils/defineResource';\nimport field from '../utils/field';\nimport {}Service from '../api/{}Service';\n\nexport default defineResource({{\n  name: {},\n  endpoint: {},\n  fields: [\n{}\n  ],\n  titleExpression: {}\n}});\n", r.name, r.name, js(&r.name), js(&r.endpoint), fields, js(r.title_expression.as_deref().unwrap_or("{id}")))
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
    call!("source", f.source.as_deref());
    call!("accept", f.accept.as_deref());
    call!("map", f.map.as_deref());
    if let Some(v) = &f.resource {
        s.push_str(&format!("\n      .resource({})", v));
    }
    if let Some(v) = &f.columns {
        s.push_str(&format!("\n      .columns({})", v));
    }
    if f.field_type != "table" {
        flag!("table", f.table);
    }
    flag!("form", f.form);
    if let Some(v) = f.length {
        s.push_str(&format!("\n      .length({})", v));
    }
    flag!("required", f.required);
    flag!("email", f.email);
    flag!("unique", f.unique);
    flag!("sortable", f.sortable);
    flag!("searchable", f.searchable);
    flag!("password", f.password);
    s
}

fn backend_code(r: &Resource, class: &str) -> String {
    let imports = "use Doctrine\\ORM\\Mapping as ORM;\nuse App\\Resource\\ResourceEntity;";
    let props = r
        .fields
        .iter()
        .map(|f| {
            format!(
                "    #[ORM\\Column{}]\n    public ?{} ${} = null;",
                if f.unique.unwrap_or(false) {
                    "(unique: true)"
                } else {
                    ""
                },
                php_type(&f.field_type),
                f.name
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    format!("<?php\n\nnamespace App\\Entity;\n\n{}\n\n#[ORM\\Entity()]\n#[ORM\\Table(name: '{}')]\nclass {} extends ResourceEntity\n{{\n    #[ORM\\Id]\n    #[ORM\\GeneratedValue]\n    #[ORM\\Column]\n    public ?int $id = null;\n\n{}\n\n    public function getTitle(): string\n    {{\n        return (string) $this->id;\n    }}\n}}\n",imports,r.name,class,props)
}
fn php_type(t: &str) -> &str {
    match t {
        "number" | "foreign" => "int",
        "boolean" => "bool",
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
                Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
                None => String::new(),
            }
        })
        .collect()
}
fn singular(s: &str) -> String {
    if s.ends_with("ies") {
        format!("{}y", &s[..s.len() - 3])
    } else if s.ends_with('s') && !s.ends_with("ses") {
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
