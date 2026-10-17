# Resource (CRUD) System

The generated admin-panel apps are **resource-driven**: a declarative resource
definition drives tables, forms, filters, validation, permissions, and row
actions. Most CRUD is expressed declaratively rather than hand-coded.

## Where it lives

| Concern | Location |
|---|---|
| Resource/field types | `templates/admin-panel/src/types/resource.ts` |
| `defineResource` | `templates/admin-panel/src/utils/defineResource.ts` |
| Field builder (`field.*`) | `templates/admin-panel/src/utils/field.ts` |
| REST service factory | `templates/admin-panel/src/api/createCrudService.ts` |
| Validation | `templates/admin-panel/src/utils/validation.ts`, `resourceValidationHooks.ts` |
| Generic CRUD page | `templates/admin-panel/src/components/resources/ResourcePage.tsx` |
| Form rendering | `components/resources/ResourceForm.tsx`, `FormField.tsx`, `FilterField.tsx`, `ViewField.tsx` |
| Grid + popups | `components/crud/ResourceGrid.tsx`, `DataTable.tsx`, `ActionFormModal.tsx` |
| Code generation | `src-tauri/src/tools/sync_schema.rs` |
| Schema read | `src-tauri/src/tools/read_schema.rs` |
| Backend base | `templates/backend/src/Resource/` (`ResourceController`, `ResourceEntity`) |

## Defining a resource (frontend)

```ts
import defineResource from '../utils/defineResource';
import field from '../utils/field';

export default defineResource({
  name: 'products',
  endpoint: '/products',            // or `service:` for a custom service
  permissions: { view: 'products.view', create: 'products.create',
                 edit: 'products.edit', delete: 'products.delete' },
  fields: [
    field.text('name').label('Name').table().form().required().searchable(),
    field.number('price').label('Price').min(0).required(),
    field.select('status').options({ active: 'Active', archived: 'Archived' }),
  ],
  actions: [ /* see "Row actions" below */ ],
});
```

- `defineResource` maps each builder through `field.config()`, lazily resolves
  string-named `resource` references to `../resources/<name>`, resolves `actions`,
  and returns a typed `Resource<T>`. If no `service` is given, it creates one from
  `endpoint` via `createCrudService`.
- The `field.*` builder is chainable: `label`, `resource`, `table`, `form`,
  `detail`, `filterable`, `required`, `minSize`/`maxSize`, `options`,
  `visibleWhen`, `min`/`max`/`scale`/`precision`/`integer`, `compute`,
  `computeExpression`, `sqlExpression`, `transforms`, `displayRules`, `lookup`,
  and validation `messages`.

### Field/behaviour defaults

`table`, `form`, and `detail` default to **true**; `filterable`, `searchable`,
`sortable`, `required`, and `unique` default to **false**. Enable explicitly when
needed.

## Consuming a resource (frontend)

`ResourcePage` is the generic CRUD page:

- begins with `'use client';`, takes `{ resource }`;
- checks permissions via `authService.hasPermission(...)` for view/create/edit/delete;
- renders the grid (`ResourceGrid`), add/edit forms (`ResourceForm`), filters
  (`ResourceFilterForm`), the detail view (`ViewField`), delete confirmation,
  and record counts;
- delegates data access to `resource.service.*`.

A page is typically just:

```tsx
'use client';
import ResourcePage from '@/components/resources/ResourcePage';
import productsResource from '@/resources/products';
export default function ProductsPage() {
  return <ResourcePage resource={productsResource} />;
}
```

## Row actions

Actions are declared on the resource `actions` array and rendered in the grid's
actions column. Each action supports `permission`, `labelExpression` (per-row
label; falsy hides the button), `confirm`, and one of:

- **Path action** — `actionExpression` evaluates to a path appended to
  `<endpoint>/<record.id>` and POSTs (e.g. `/contacted` → `POST /api/leads/42/contacted`).
- **Handler action** — `handler` runs custom JavaScript with an injected helper
  API (`data`/`record`, `api`, `message`, `modal`, `refresh`, `openForm`, `close`,
  `values`).
- **Form action** — `form` opens a reusable dialog (`ActionFormModal`) built from
  a `fields` list and submits to a custom endpoint (`{id}` supported), then
  refreshes the grid.

Frontend visibility is **not** authorization — backend routes must re-check
permission and record state.

## Schema-driven generation (`sync_schema`)

`SyncSchemaTool` (`src-tauri/src/tools/sync_schema.rs`) turns a schema
description into real code:

- For each resource it writes `frontend/src/resources/<name>.ts` (via
  `frontend_code()` → `field_js()` / `actions_js()` / `action_form_js()`) and
  `backend/src/Entity/<Class>.php` (via `backend_code()`).
- Actions (including handler/form fields) are serialized back into the generated
  TS, so **declare actions through `sync_schema`** rather than hand-editing
  generated files (manual edits are overwritten on the next sync).
- It runs migrations after generation.
- `read_schema` reads the written schema JSON (`schema_file_path`).

Relevant helpers: `pascal()`, `singular()`, `php_type()`, `save_schemas()`.

## Validation

- Declarative field rules (required/unique/email/phone/min/max/options) are built
  into the schema.
- Custom cross-field rules use mirrored hooks: backend
  `Resource/Hooks/<resource>/XxxHook.php` (`ValidatesResource`) and frontend
  `resources/hooks/<resource>/xxxHook.ts`, registered in
  `resources/hooks/index.ts`. Both must implement the same logical rule.

## Related documentation

The agent-facing instructions for building resources live in Eggshell's runtime
skills `crud_creation` (with `row_actions` and `custom_ui` documents) under
`src-tauri/src/skills/`. See [`skill-system.md`](./skill-system.md).
