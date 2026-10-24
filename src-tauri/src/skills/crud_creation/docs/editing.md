# Editing an Existing Schema

Do **not** re-emit an entire resource just to add, remove, or change a field. When
a resource already exists, use the `operations` array on `sync_schema`. Operations
are applied in order to the stored schema, then only the affected resources are
regenerated (frontend resource file, backend entity) and migrations are run once.

Read the current state first if the resource is not fully known:

```javascript
read_schema({ resource: "products" })
```

### 4a.1 Operation shape

Every operation has an `action` and a `resource` name. Other properties depend on
the action.

| Action | Required properties | Purpose |
|---|---|---|
| `addField` | `field` (full field object) + optional `before` / `after` / `index` | Add a new field |
| `updateField` | `field` (existing name) + `changes` and/or `rename` | Change properties, or rename |
| `removeField` | `field` (existing name) | Delete a field (drops its column) |
| `moveField` | `field` (existing name) + `before` / `after` / `index` | Reorder a field |
| `updateResource` | `changes` | Change resource-level properties (`endpoint`, `titleExpression`, `actions`) |
| `removeResource` | — | Remove the resource from the schema |

Insertion position rules for `addField` and `moveField`:

- omit all anchors to append at the end;
- `before: "<fieldName>"` inserts before that field;
- `after: "<fieldName>"` inserts after that field;
- `index: <n>` inserts at a zero-based position;
- use **only one** anchor per operation.

Matching is case-insensitive for resource and field names.

## `changes` semantics

`changes` is a camelCase object whose keys are the same properties documented in
section 2 (for example `label`, `required`, `searchable`, `options`, `scale`,
`visibleWhen`).

- setting a property replaces it;
- setting a property to `null` removes it (cosmetic absence means the documented
  default, not `false`);
- `name` and `type` cannot be removed through `changes`.

Renaming is done with `rename`, not `changes.name`:

```javascript
sync_schema({
  operations: [
    { action: "updateField", resource: "products", field: "price", rename: "unitPrice" }
  ]
})
```

Renaming a field drops the old column and creates a new one, so existing data in
that column is not migrated. Prefer renaming only during development, or add the
new field and copy/backfill data with an explicit migration.

## Examples

Add a field to an existing resource:

```javascript
sync_schema({
  operations: [
    {
      action: "addField",
      resource: "products",
      field: {
        name: "stockQuantity",
        type: "number",
        label: "Stock Quantity",
        integer: true,
        min: 0,
        required: true,
        default: 0
      },
      after: "price"
    }
  ]
})
```

Update several fields at once:

```javascript
sync_schema({
  operations: [
    { action: "updateField", resource: "products", field: "name", changes: { searchable: true, sortable: true } },
    { action: "updateField", resource: "products", field: "price", changes: { type: "decimal", scale: 2, min: 0 } },
    { action: "updateField", resource: "products", field: "internalNotes", changes: { table: false } }
  ]
})
```

Remove a field and reorder another:

```javascript
sync_schema({
  operations: [
    { action: "removeField", resource: "products", field: "legacyCode" },
    { action: "moveField", resource: "products", field: "sku", before: "name" }
  ]
})
```

Change resource-level behavior:

```javascript
sync_schema({
  operations: [
    {
      action: "updateResource",
      resource: "leads",
      changes: {
        titleExpression: "data.name",
        actions: [
          {
            permission: "leads.edit",
            actionExpression: "data.status == 'new' ? '/contacted' : null",
            labelExpression: "data.status == 'new' ? 'Mark Contacted' : null",
            confirm: true
          }
        ]
      }
    }
  ]
})
```

`updateResource` with `actions` **replaces** the whole `actions` array. To keep
existing actions, read the resource first and emit the complete list.

## Mixing full definitions and operations

`resources` and `operations` may be combined in one call. Full definitions are
applied first, then operations. Only resources that were actually created,
replaced, or edited are regenerated.

```javascript
sync_schema({
  resources: [ /* new resources only */ ],
  operations: [ /* edits to existing resources */ ]
})
```

### What not to do

- Do not hand-edit generated resource/entity files; the next sync overwrites them.
- Do not rename resources through `updateResource`; create the new resource and
  remove the old one.
- Do not use `changes` to add or remove fields; use the field actions.

### After editing

After any schema edit:

1. Uupdate custom controllers, hooks, pages, and menu entries that reference the changed field names;
3. update any validation hooks that read the renamed field;
4. run `lint_code`;