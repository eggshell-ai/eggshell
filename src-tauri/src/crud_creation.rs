use super::{Skill};

// Paste the full CRUD skill instructions in this constant.
const CONTENT: &str = "# CRUD Creation Skill

This skill explains how to create a complete CRUD (Create, Read, Update, Delete) interface in the admin panel system using the declarative resource system, `ResourceController`, `ResourcePage`, validation hooks, and the provided project-writing tools.

The system is intentionally resource-driven. A well-defined resource should generate most ordinary CRUD behavior automatically. Do not build custom forms, tables, filters, validation rendering, delete dialogs, or list pages when the resource system already provides them.

The preferred approach is:

1. Model the requirement as a resource.
2. Express all ordinary behavior declaratively in `sync_schema`.
3. Add mirrored frontend/backend validation hooks only for rules that cannot be represented by field properties.
4. Create the backend `ResourceController` using `write_file`.
5. Add custom backend routes only for resource actions or other behavior that cannot be declarative.
6. Create the frontend page with `ResourcePage`.
7. Add the menu entry.
8. Audit every user requirement against the implementation before finishing.

---

## 1. Mental Model / Architecture

A CRUD feature is not normally a hand-built page. It is a resource definition plus a thin backend controller and a thin frontend page.

The resource definition is the source of truth for the ordinary data-management behavior of the feature.

### What `sync_schema` does

Use `sync_schema` first.

A resource definition supplied to `sync_schema` describes:

- the resource name;
- the API endpoint;
- the fields;
- field visibility in tables, forms, and detail views;
- required and unique fields;
- default values;
- searchability;
- filtering;
- sorting;
- select options;
- conditional field visibility;
- custom row actions;
- ordinary field validation.

From that definition, the system generates or updates the corresponding resource metadata and schema-related artifacts, including:

- frontend resource definition;
- backend entity;
- database migration/schema changes;
- backend validation corresponding to supported declarative field rules.

Do not manually duplicate behavior already represented by the resource.

### What `ResourceController` does

A backend controller extending `ResourceController` provides the normal CRUD API for the resource, including:

- list;
- view;
- create;
- update;
- delete;
- search;
- filtering;
- sorting;
- validation handling.

The resource controller should usually contain only:

- `getEntityClass()`;
- `getResourceName()`;
- explicitly required custom routes.

Use `write_file` to create the controller. Do not use `write_controller`.

### What `ResourcePage` does

A frontend page using `ResourcePage` automatically provides the standard CRUD UI, including:

- data table;
- current filtered record count;
- search;
- filters;
- sorting where enabled;
- add form;
- edit form;
- detail/view action;
- validation messages;
- delete confirmation;
- permission-aware standard actions;
- bulk operations supported by the resource system.

The count shown by `ResourcePage` reflects the currently selected search/filter state. If a requirement says things such as:

- \"show the number of leads\";
- \"show the number of matching records\";
- \"show a count for the selected filters\";

do not create a custom counter. The standard `ResourcePage` count already covers this behavior.

### When custom code is justified

Use custom code only when the requested behavior cannot be represented by the normal resource configuration.

Typical reasons include:

- cross-field validation;
- conditional-required validation;
- state/workflow transitions;
- external API calls;
- multi-record domain operations;
- custom row actions;
- specialized UI behavior not supported by `ResourcePage`.

Even when custom code is needed, keep the standard resource/page/controller architecture and add the smallest extension required.

---

## 2. Exact Resource Schema Reference

Call `sync_schema` with a `resources` array.

Basic shape:

```javascript
sync_schema({
  resources: [
    {
      name: \"products\",
      endpoint: \"/products\",
      fields: [
        {
          name: \"name\",
          type: \"text\",
          label: \"Product Name\"
        }
      ]
    }
  ]
})
```

An optional `projectPath` may be supplied when a specific project must be targeted.

---

### 2.1 Resource properties

A resource object supports the following core properties.

| Property | Required | Default / omission behavior | Meaning |
|---|---:|---|---|
| `name` | yes | none | Resource identifier. Use a stable plural resource name such as `products`, `customers`, or `leads`. |
| `endpoint` | yes for normal CRUD | none | API endpoint such as `/products`. Required for standard CRUD and row actions. |
| `fields` | yes | none | Array of field definitions. |
| `actions` | no | `[]` / no custom actions | Custom row actions shown by the resource grid. |
| `titleExpression` | no | unset | JavaScript expression/template used when a resource-specific title is needed. Do not add it unless useful. |

Keep naming consistent across:

- resource name;
- endpoint;
- entity;
- controller;
- page import;
- hook registry key;
- permissions;
- menu route.

---

### 2.2 Field types

Use one of the following field types:

| Type | Intended use |
|---|---|
| `\"text\"` | Single-line text input |
| `\"textarea\"` | Multi-line text input |
| `\"email\"` | Email input with standard email validation |
| `\"phone\"` | Phone input with permissive local/international phone validation |
| `\"password\"` | Password input |
| `\"number\"` | Numeric input |
| `\"boolean\"` | Boolean / checkbox input |
| `\"date\"` | Date picker |
| `\"file\"` | File upload |
| `\"select\"` | Dropdown; use `options` for static choices or `resource` for dynamic choices |
| `\"tags\"` | Multi-select tags, normally paired with `resource` |
| `\"table\"` | Line-item grid, paired with `columns` |

Do not invent field types. If a prompt requests a UI concept that can be represented by an existing field type, use the existing type.

Examples:

- \"notes\" or \"description\" with multiple lines → `textarea`;
- status/source/category choices → `select`;
- yes/no or active/inactive → `boolean`;
- uploaded image → `file`.

---

### 2.3 Field properties and defaults

Every field must have `name` and `type`.

The behavioral defaults are important. Do not assume that omission means `false`.

| Property | Default | Meaning |
|---|---|---|
| `name` | none; required | Field identifier |
| `type` | none; required | Field type |
| `label` | unset | Display label. Set explicitly when the wording matters. |
| `table` | `true` | Show in the standard table |
| `form` | `true` | Show in create/edit forms |
| `detail` | `true` | Show in the standard detail view |
| `filterable` | `false` | Generate a standard filter for this field |
| `searchable` | `false` | Include this field in standard server-side search |
| `sortable` | `false` | Allow standard sorting by this field |
| `required` | `false` | Field is not required unless explicitly enabled |
| `unique` | `false` | No uniqueness constraint unless explicitly enabled |
| `default` | unset | No new-record default |
| `messages` | unset | Use standard generated validation messages |
| `trueLabel` | unset | No custom true label |
| `falseLabel` | unset | No custom false label |
| `accept` | unset | No field-specific file accept restriction |
| `minSize` | unset | No minimum-size rule |
| `maxSize` | unset | No maximum-size rule |
| `resource` | unset | No remote/dynamic select resource |
| `options` | unset | No static select options |
| `visibleWhen` | unset | Field is not conditionally hidden |
| `columns` | unset | No child table columns |

\"Unset\" means the property is omitted from the resource unless the requirement needs it. Do not invent a semantic value for an omitted optional property.

Because `table`, `form`, and `detail` default to `true`, this is valid:

```javascript
{
  name: \"name\",
  type: \"text\",
  label: \"Name\",
  required: true,
  searchable: true
}
```

There is no need to redundantly add:

```javascript
table: true,
form: true,
detail: true
```

unless explicitness is useful for the task.

Conversely, `searchable`, `filterable`, and `sortable` default to `false`, so they must be enabled when the requirement needs them.

---

### 2.4 Static selects

Use `type: \"select\"` with `options`.

The option keys are stored values. The option values are display labels.

```javascript
{
  name: \"status\",
  type: \"select\",
  label: \"Status\",
  options: {
    new: \"New\",
    contacted: \"Contacted\",
    qualified: \"Qualified\",
    lost: \"Lost\"
  },
  default: \"new\",
  required: true,
  filterable: true
}
```

The stored value must be one of the option keys, or empty when the field is not required.

Use stable machine-friendly keys and human-friendly labels.

Good:

```javascript
options: {
  trade_show: \"Trade Show\"
}
```

Avoid storing the presentation label when a stable machine key is more appropriate.

For dynamic choices backed by another resource, use `type: \"select\"` with `resource`.

---

### 2.5 Defaults

Use `default` for new-record defaults.

```javascript
{
  name: \"status\",
  type: \"select\",
  options: {
    new: \"New\",
    contacted: \"Contacted\"
  },
  default: \"new\"
}
```

Defaults apply when creating a record.

They must not overwrite an existing value while editing a record.

---

### 2.6 Boolean labels

Use `trueLabel` and `falseLabel` when boolean values need domain-specific display labels.

```javascript
{
  name: \"active\",
  type: \"boolean\",
  label: \"Status\",
  default: true,
  trueLabel: \"Active\",
  falseLabel: \"Inactive\",
  filterable: true
}
```

These labels will be used by the resource UI in tables, forms, filters, and detail views.

---

### 2.7 Expressions

Expression-valued resource properties use JavaScript expressions.

`data` is the standard record/form object available to the expression.

Expressions intentionally support ordinary JavaScript expression syntax, including core operators and common built-in libraries such as `Math`.

Examples:

```javascript
visibleWhen: \"data.source == 'other'\"
```

```javascript
visibleWhen: \"data.type === 'company' && data.active\"
```

```javascript
actionExpression: \"data.status == 'new' ? '/contacted' : null\"
```

```javascript
labelExpression: \"data.status == 'new' ? 'Mark Contacted' : null\"
```

```javascript
visibleWhen: \"Number(data.total || 0) > 0\"
```

```javascript
visibleWhen: \"Math.abs(Number(data.balance || 0)) > 100\"
```

```javascript
labelExpression: \"'Mark Contacted'\"
```

Core JavaScript operators may be used as appropriate, including:

- equality and inequality: `==`, `!=`, `===`, `!==`;
- comparisons: `<`, `<=`, `>`, `>=`;
- logical operators: `&&`, `||`, `!`;
- arithmetic operators;
- nullish/defaulting expressions where supported by normal JavaScript syntax;
- ternary expressions: `condition ? a : b`;
- property access through `data`;
- common JavaScript built-ins such as `Number`, `String`, and `Math`.

Keep expressions short and deterministic.

Do not put business-critical authorization or validation exclusively in an expression. Expressions control frontend/resource behavior; the backend remains authoritative.

---

### 2.8 Conditional visibility

Use `visibleWhen` when a field should be displayed only under a condition.

Example:

```javascript
{
  name: \"sourceDescription\",
  type: \"text\",
  label: \"Other Source Description\",
  visibleWhen: \"data.source == 'other'\"
}
```

This means the field is visible only when `source` equals `\"other\"`.

`visibleWhen` controls frontend visibility only.

It does **not** make the field conditionally required on the backend. If a field is required only under a condition, use mirrored frontend/backend validation hooks as described later.

---

## 3. Requirement → Feature Mapping

Before writing code, translate the prompt into resource-system features.

Use this mapping as the default interpretation.

| Prompt requirement | Implementation |
|---|---|
| \"Create/manage X records\" | Resource + `ResourceController` + `ResourcePage` |
| \"X is required\" | `required: true` |
| \"X must be unique\" | `unique: true` |
| \"X defaults to Y\" | `default: Y` |
| \"Search by X\" | `searchable: true` |
| \"Filter by X\" | `filterable: true` |
| \"Sort by X\" | `sortable: true` |
| \"Show X in table\" | Already `table: true` by default |
| \"Do not show X in table\" | `table: false` |
| \"Show X in form\" | Already `form: true` by default |
| \"Do not edit X through form\" | `form: false` |
| \"Show X in detail view\" | Already `detail: true` by default |
| \"Do not show X in detail view\" | `detail: false` |
| \"Multi-line notes\" | `type: \"textarea\"` |
| \"Choose from fixed values\" | `type: \"select\"` + `options` |
| \"Show B if A == X\" | `visibleWhen: \"data.A == 'X'\"` |
| \"If A == X, B is required\" | `visibleWhen` + mirrored validation hooks |
| \"At least one of A or B\" | Mirrored frontend/backend validation hooks |
| \"Custom per-row button\" | Resource `actions` + backend POST route |
| \"Only show action in state X\" | `actionExpression` returning path or `null` |
| \"Confirm custom action\" | `confirm: true` |
| \"Confirm deletion\" | Built into `ResourcePage`; do not add custom confirmation |
| \"Show matching record count\" | Built into `ResourcePage` |
| \"Show count for selected filters\" | Built into `ResourcePage` and follows active filters/search |
| \"Standard table/forms/view\" | Built into `ResourcePage` |
| \"Helpful validation messages\" | Built in; use `messages` only if custom wording is specifically required |
| \"Backend CRUD\" | Standard `ResourceController` |
| \"Cross-field business rule\" | Validation hooks |
| \"Workflow/state mutation\" | Custom action/backend route plus backend state check |

### Important filter rule

If the prompt says \"filter by X\", explicitly set:

```javascript
filterable: true
```

Do not assume that a select is automatically filterable.

Example:

```javascript
{
  name: \"source\",
  type: \"select\",
  options: {
    site: \"Website\",
    referral: \"Referral\",
    phone: \"Phone Call\",
    trade: \"Trade Show\",
    other: \"Other\"
  },
  filterable: true
}
```

and:

```javascript
{
  name: \"status\",
  type: \"select\",
  options: {
    new: \"New\",
    contacted: \"Contacted\",
    qualified: \"Qualified\",
    lost: \"Lost\"
  },
  filterable: true
}
```

---

## 4. Standard CRUD Workflow

Use the following sequence for an ordinary CRUD feature.

### Step 1 — Extract requirements

Identify:

- resource name;
- endpoint;
- fields;
- field types;
- required fields;
- unique fields;
- defaults;
- searchable fields;
- filterable fields;
- sortable fields;
- conditional visibility;
- cross-field validation;
- custom actions;
- permissions;
- page route;
- menu location.

Do not start coding before distinguishing declarative requirements from custom behavior.

### Step 2 — Call `sync_schema`

Define the resource and all behavior that can be declarative.

Example:

```javascript
sync_schema({
  resources: [
    {
      name: \"products\",
      endpoint: \"/products\",
      fields: [
        {
          name: \"name\",
          type: \"text\",
          label: \"Product Name\",
          required: true,
          searchable: true,
          sortable: true
        },
        {
          name: \"sku\",
          type: \"text\",
          label: \"SKU\",
          required: true,
          unique: true,
          searchable: true
        },
        {
          name: \"price\",
          type: \"number\",
          label: \"Price\",
          required: true,
          sortable: true
        },
        {
          name: \"active\",
          type: \"boolean\",
          label: \"Status\",
          default: true,
          trueLabel: \"Active\",
          falseLabel: \"Inactive\",
          filterable: true
        }
      ]
    }
  ]
})
```

### Step 3 — Add validation hooks only when necessary

If all validation is ordinary field-level validation, do not create hooks.

Create hooks when the requirement is cross-field or conditional.

Whenever a custom validation rule exists:

- create the backend validation hook;
- create the matching frontend validation hook;
- register the frontend hook safely;
- keep both implementations semantically equivalent.

### Step 4 — Create controller with `write_file`

Always use `write_file`.

Standard controller:

```javascript
write_file({
  shell: \"backend\",
  path: \"Controller/ProductController.php\",
  content: `<?php

namespace App\\Controller;

use App\\Entity\\Product;
use App\\Resource\\ResourceController;
use Symfony\\Component\\Routing\\Attribute\\Route;

#[Route('/api/products', name: 'products.')]
final class ProductController extends ResourceController
{
    protected function getEntityClass(): string
    {
        return Product::class;
    }

    protected function getResourceName(): string
    {
        return 'products';
    }
}
`
})
```

Do not use `write_controller`.

If custom routes are required, add them to this same controller file.

### Step 5 — Create frontend page

Use `write_page` and `ResourcePage`.

```javascript
write_page({
  route: \"/products\",
  code: `'use client';

import ResourcePage from '@/components/resources/ResourcePage';
import productsResource from '@/resources/products';

export default function ProductsPage() {
  return <ResourcePage resource={productsResource} />;
}
`
})
```

Do not build a separate CRUD table/form implementation.

### Step 6 — Add menu entry

```javascript
write_menu({
  name: \"Inventory.Products\",
  route: \"/products\",
  icon: \"ShoppingOutlined\",
  after: \"Dashboard\"
})
```

### Step 7 — Verify the prompt requirement-by-requirement

Do not finish immediately after the last tool call.

Perform the audit described in the final section of this skill.

---

## 5. Validation

Validation has two layers:

1. declarative validation;
2. custom validation hooks.

Use the simplest layer that correctly expresses the rule.

Backend validation is authoritative. Frontend validation exists for fast user feedback.

---

### 5.1 Simple declarative validation

Use field properties for ordinary single-field validation.

Example:

```javascript
{
  name: \"fullName\",
  type: \"text\",
  required: true,
  minSize: 2
}
```

```javascript
{
  name: \"email\",
  type: \"email\",
  unique: true
}
```

```javascript
{
  name: \"phone\",
  type: \"phone\"
}
```

Supported ordinary behavior includes:

- `required: true`;
- `unique: true`;
- email validation via `type: \"email\"`;
- phone validation via `type: \"phone\"`;
- `minSize`;
- `maxSize`;
- static select option validation;
- custom field messages through `messages`.

Use database/backend enforcement for uniqueness. Do not rely only on frontend validation.

Standard validation messages are automatically provided. Only define `messages` when the user explicitly requires specific wording.

Example:

```javascript
{
  name: \"email\",
  type: \"email\",
  required: true,
  unique: true,
  messages: {
    required: \"Email is required.\",
    email: \"Enter a valid email address.\",
    unique: \"This email address is already in use.\"
  }
}
```

Backend validation errors must remain field-addressable so `ResourcePage` can show them beside the corresponding input.

---

### 5.2 Backend validation hooks

For validation that cannot be represented declaratively, create a hook under:

```text
src/Resource/Hooks/<resourceName>/
```

With `write_file`, the backend-relative path is:

```text
Resource/Hooks/<resourceName>/<ClassName>.php
```

The namespace is:

```php
App\\Resource\\Hooks\\<resourceName>
```

The hook implements `ValidatesResource`.

Shape:

```php
<?php

namespace App\\Resource\\Hooks\\orders;

use App\\Resource\\Hook\\ValidatesResource;

final class OrderValidationHook implements ValidatesResource
{
    public function validate(object $entity, array $data, string $action): array
    {
        $errors = [];

        // custom rules

        return $errors;
    }
}
```

The hook runs during `store` and `update` after standard declarative validation.

Return:

```php
[
    'fieldName' => 'Error message'
]
```

or arrays of messages if needed.

Return an empty array when validation passes.

Custom validation errors produce the normal validation response and should be displayed by `ResourcePage` beside the relevant field.

Do not put resource validation rules in controller actions merely because custom code is needed. Validation hooks are the designated mechanism.

---

### 5.3 Frontend validation hooks

Mirror every backend custom validation rule in the frontend.

Create the hook under:

```text
src/resources/hooks/<resourceName>/
```

With `write_file`, the frontend-relative path is:

```text
resources/hooks/<resourceName>/<hookName>.ts
```

Example:

```typescript
import type { ResourceValidationHook } from '../../../types/resource';

const orderValidationHook: ResourceValidationHook = {
  validate: (data, action, record) => {
    const errors: Record<string, string | string[]> = {};

    // mirror backend rules

    return errors;
  },
};

export default orderValidationHook;
```

The hook receives:

```text
(data, action, record)
```

where:

- `data` is the submitted form data;
- `action` is `\"store\"` or `\"update\"`;
- `record` is the existing record on update.

The frontend hook is for immediate feedback.

The backend hook is authoritative.

They must implement the same logical rule.

---

### 5.4 Validation registry safety

After creating a frontend validation hook, register it in:

```text
src/resources/hooks/index.ts
```

This registry may already contain hooks for other resources.

**Never overwrite it from memory.**

Required sequence:

1. Read the current registry.
2. Preserve all existing imports and registrations.
3. Add the new import/registration.
4. Overwrite the file with the full updated registry.

Tool sequence:

```javascript
read_file({
  shell: \"frontend\",
  path: \"resources/hooks/index.ts\"
})
```

Then:

```javascript
write_file({
  shell: \"frontend\",
  path: \"resources/hooks/index.ts\",
  content: \"<full existing registry plus the new hook>\"
})
```

Example final registry:

```typescript
import type { ResourceValidationHook } from '../../types/resource';
import userValidationHook from './users/userValidationHook';
import invoiceValidationHook from './invoices/invoiceValidationHook';
import leadValidationHook from './leads/leadValidationHook';

const validationHooks: Record<string, ResourceValidationHook[]> = {
  users: [userValidationHook],
  invoices: [invoiceValidationHook],
  leads: [leadValidationHook],
};

export default validationHooks;
```

Do not delete unrelated hook registrations.

When modifying an existing hook, read it if necessary, then write the complete updated file. Keep backend and frontend mirrors synchronized.

---

### 5.5 Cross-field validation recipe: at least one of email or phone

This pattern is common and must be implemented carefully.

Requirement:

> At least one of email or phone must be provided.

Do not write checks that accidentally accept both fields being absent.

Normalize empty strings and whitespace.

#### Backend

```javascript
write_file({
  shell: \"backend\",
  path: \"Resource/Hooks/leads/LeadValidationHook.php\",
  content: `<?php

namespace App\\Resource\\Hooks\\leads;

use App\\Resource\\Hook\\ValidatesResource;

final class LeadValidationHook implements ValidatesResource
{
    public function validate(object $entity, array $data, string $action): array
    {
        $errors = [];

        $email = trim((string) ($entity->email ?? ''));
        $phone = trim((string) ($entity->phone ?? ''));

        if ($email === '' && $phone === '') {
            $errors['email'] = 'Enter at least an email address or phone number.';
            $errors['phone'] = 'Enter at least an email address or phone number.';
        }

        return $errors;
    }
}
`
})
```

#### Frontend

```javascript
write_file({
  shell: \"frontend\",
  path: \"resources/hooks/leads/leadValidationHook.ts\",
  content: `import type { ResourceValidationHook } from '../../../types/resource';

const leadValidationHook: ResourceValidationHook = {
  validate: (data, action, record) => {
    const errors: Record<string, string | string[]> = {};

    const email = String(data?.email ?? '').trim();
    const phone = String(data?.phone ?? '').trim();

    if (!email && !phone) {
      const message = 'Enter at least an email address or phone number.';
      errors.email = message;
      errors.phone = message;
    }

    return errors;
  },
};

export default leadValidationHook;
`
})
```

The two checks are intentionally equivalent:

```text
backend: email == empty AND phone == empty
frontend: !email AND !phone
```

Do not use an expression such as:

```javascript
!(data?.email || data.phone == null)
```

because that changes the logical meaning and can accept invalid input.

---

### 5.6 Conditional-required recipe

Requirement:

> If source is `other`, show `sourceDescription` and require it.

This requires two parts:

1. conditional frontend visibility;
2. frontend + backend validation.

#### Resource field

```javascript
{
  name: \"sourceDescription\",
  type: \"text\",
  label: \"Other Source Description\",
  visibleWhen: \"data.source == 'other'\"
}
```

Do **not** set:

```javascript
required: true
```

if the field is required only under that condition, because unconditional `required: true` would make it required for every source.

Instead, enforce the condition with hooks.

#### Backend rule

```php
$source = (string) ($entity->source ?? '');
$description = trim((string) ($entity->sourceDescription ?? ''));

if ($source === 'other' && $description === '') {
    $errors['sourceDescription'] = 'Describe the other lead source.';
}
```

#### Frontend rule

```typescript
const source = String(data?.source ?? '');
const description = String(data?.sourceDescription ?? '').trim();

if (source === 'other' && !description) {
  errors.sourceDescription = 'Describe the other lead source.';
}
```

These rules must remain equivalent.

Conditional visibility is not validation. A malicious or custom client can call the backend without respecting `visibleWhen`, so the backend hook must enforce the rule.

---

## 6. Filtering, Search, Sorting, and Count

These are standard resource capabilities. Prefer declarative configuration.

---

### 6.1 Search

Set:

```javascript
searchable: true
```

on every field that should participate in standard search.

Example:

```javascript
{
  name: \"name\",
  type: \"text\",
  searchable: true
}
```

```javascript
{
  name: \"email\",
  type: \"email\",
  searchable: true
}
```

Search is server-side and matches across the fields configured as searchable.

If the prompt says:

> Search leads by name, email, or phone.

Then all three fields need:

```javascript
searchable: true
```

Do not implement a custom search endpoint for this.

---

### 6.2 Filtering

`filterable` defaults to `false`.

When a requirement says \"filter by X\", explicitly set:

```javascript
filterable: true
```

This includes select fields.

Example:

```javascript
{
  name: \"status\",
  type: \"select\",
  label: \"Status\",
  options: {
    new: \"New\",
    contacted: \"Contacted\",
    qualified: \"Qualified\",
    lost: \"Lost\"
  },
  default: \"new\",
  required: true,
  filterable: true
}
```

Example:

```javascript
{
  name: \"source\",
  type: \"select\",
  label: \"Lead Source\",
  options: {
    site: \"Website\",
    referral: \"Referral\",
    phone: \"Phone Call\",
    trade: \"Trade Show\",
    other: \"Other\"
  },
  default: \"site\",
  required: true,
  filterable: true
}
```

Do not assume select fields automatically produce filters.

---

### 6.3 Sorting

`sortable` defaults to `false`.

If the prompt requires sorting by a field, set:

```javascript
sortable: true
```

Example:

```javascript
{
  name: \"name\",
  type: \"text\",
  sortable: true
}
```

Use standard resource sorting instead of custom client-side sorting.

---

### 6.4 Filtered count

`ResourcePage` automatically shows the count for the records currently matching the active resource query.

The count follows:

- selected filters;
- current search;
- normal resource query state.

Therefore a requirement such as:

> Show the number of leads currently matching the selected filters.

requires no custom field, endpoint, component, or hook.

Implement the filters correctly and use the standard `ResourcePage`.

If the implementation uses a custom page instead of `ResourcePage`, the automatic count guarantee no longer applies. Do not switch to a custom page unless the prompt truly requires unsupported UI behavior.

---

## 7. Custom Row Actions

Use resource `actions` for row-level domain operations such as:

- activate/deactivate;
- mark contacted;
- approve;
- cancel;
- reopen.

A custom row action consists of:

1. frontend/resource action declaration;
2. backend POST route;
3. matching permission enforcement;
4. backend state/business-rule validation.

Frontend visibility is convenience. Backend checks are security and correctness.

---

### 7.1 Frontend action declaration

Example:

```javascript
actions: [
  {
    permission: \"leads.edit\",
    actionExpression: \"data.status == 'new' ? '/contacted' : null\",
    labelExpression: \"data.status == 'new' ? 'Mark Contacted' : null\",
    confirm: true
  }
]
```

Behavior:

- `permission` controls whether the action is available to the user in the UI;
- `actionExpression` is evaluated for each row with the row available as `data`;
- its result is appended to `<endpoint>/<record.id>`;
- a `null`/falsy action result hides the action for that row;
- `labelExpression` determines the displayed label and may also return a falsy value when no button should be shown;
- `confirm: true` shows a confirmation dialog;
- the action uses POST;
- the grid refreshes after a successful action.

For:

```javascript
endpoint: \"/leads\"
```

record id `42`, and:

```javascript
actionExpression: \"'/contacted'\"
```

the request is:

```text
POST /api/leads/42/contacted
```

through the normal backend resource route.

---

### 7.2 Frontend visibility is not backend enforcement

This is a critical invariant.

Suppose the action is only visible when:

```javascript
data.status == 'new'
```

A client can still manually send:

```text
POST /api/leads/42/contacted
```

even if the record is already `qualified`, `lost`, or `contacted`.

Therefore the backend route must verify the same state rule.

Do not treat:

```javascript
actionExpression: \"data.status == 'new' ? '/contacted' : null\"
```

as authorization or validation.

It is UI behavior only.

---

### 7.3 Permission invariant

Unless a task deliberately specifies a different backend permission model, the permission declared on the resource action must exactly match the permission checked by the backend route.

If the resource says:

```javascript
permission: \"leads.edit\"
```

the backend must check:

```php
$this->denyAccessUnlessGranted('leads.edit');
```

Do not accidentally check an unrelated permission such as:

```php
$this->denyAccessUnlessGranted('users.edit');
```

Before finishing, compare the permission strings character-for-character.

If a different backend permission is intentionally required, document the reason explicitly rather than silently diverging.

---

### 7.4 Backend custom route

Create the resource controller using `write_file`, and add the custom route to it.

Example:

```javascript
write_file({
  shell: \"backend\",
  path: \"Controller/LeadController.php\",
  content: `<?php

namespace App\\Controller;

use App\\Entity\\Lead;
use App\\Resource\\ResourceController;
use Symfony\\Component\\HttpFoundation\\JsonResponse;
use Symfony\\Component\\HttpKernel\\Exception\\BadRequestHttpException;
use Symfony\\Component\\HttpKernel\\Exception\\NotFoundHttpException;
use Symfony\\Component\\Routing\\Attribute\\Route;

#[Route('/api/leads', name: 'leads.')]
final class LeadController extends ResourceController
{
    protected function getEntityClass(): string
    {
        return Lead::class;
    }

    protected function getResourceName(): string
    {
        return 'leads';
    }

    #[Route('/{id}/contacted', name: 'contacted', requirements: ['id' => '\\d+'], methods: ['POST'])]
    public function contacted(int $id): JsonResponse
    {
        $this->denyAccessUnlessGranted('leads.edit');

        $record = $this->entityManager
            ->getRepository($this->getEntityClass())
            ->find($id);

        if (!$record) {
            throw new NotFoundHttpException('Record not found');
        }

        if ($record->status !== 'new') {
            throw new BadRequestHttpException('Only new leads can be marked as contacted.');
        }

        $record->status = 'contacted';
        $this->entityManager->flush();

        return $this->json($record);
    }
}
`
})
```

This route checks all three essential things:

1. permission;
2. record existence;
3. valid current state.

Only then does it mutate state.

---

### 7.5 State validation rule

Every state-dependent row action should ask:

> What must be true about the current record for this transition to be valid?

Examples:

- Mark Contacted → current status must be `new`;
- Approve → current status must be `draft`;
- Cancel → current status must be one of the cancellable states;
- Reactivate → current status must be `inactive`.

Mirror this state in the frontend expression for good UX, but enforce it again on the backend.

---

## 8. Controller, Page, and Menu Creation

---

### 8.1 Writing Controllers
Use:

```javascript
write_file({
  shell: \"backend\",
  path: \"Controller/<EntityName>Controller.php\",
  content: \"...\"
})
```

Example:

```javascript
write_file({
  shell: \"backend\",
  path: \"Controller/CustomerController.php\",
  content: `<?php

namespace App\\Controller;

use App\\Entity\\Customer;
use App\\Resource\\ResourceController;
use Symfony\\Component\\Routing\\Attribute\\Route;

#[Route('/api/customers', name: 'customers.')]
final class CustomerController extends ResourceController
{
    protected function getEntityClass(): string
    {
        return Customer::class;
    }

    protected function getResourceName(): string
    {
        return 'customers';
    }
}
`
})
```

Use the standard shell/path convention shown above.

Add imports required by custom routes. Do not use a class such as `NotFoundHttpException` without importing it.

Keep standard CRUD behavior inherited from `ResourceController`.

---

### 8.2 Frontend page

Use `write_page`.

Example:

```javascript
write_page({
  route: \"/customers\",
  code: `'use client';

import ResourcePage from '@/components/resources/ResourcePage';
import customersResource from '@/resources/customers';

export default function CustomersPage() {
  return <ResourcePage resource={customersResource} />;
}
`
})
```

Do not manually build:

- a table;
- create/edit modal;
- filter widgets;
- validation display;
- delete confirmation;
- count display;

when `ResourcePage` already supplies them.

---

### 8.3 Menu

Use `write_menu`.

Example:

```javascript
write_menu({
  name: \"CRM.Customers\",
  route: \"/customers\",
  icon: \"TeamOutlined\",
  after: \"Dashboard\"
})
```

Choose a menu hierarchy and icon appropriate to the task. Keep the route identical to the page route.

---

## 9. Complete Worked Example: Sales Lead Tracker

This example intentionally combines the common features that are easy to implement incorrectly when shown only in isolation:

- defaults;
- search;
- filters;
- conditional field visibility;
- conditional-required validation;
- cross-field validation;
- custom row action;
- custom action permission;
- backend state enforcement;
- registry-safe frontend validation;
- standard controller/page/menu;
- automatic filtered count.

### Requirement

Build a sales lead tracker with:

- name;
- email;
- phone;
- lead source;
- an \"Other Source Description\" field visible only when source is `other`;
- the description required when source is `other`;
- status;
- notes;
- at least one of email or phone required;
- default source of Website;
- default status of New;
- search by name, email, and phone;
- filter by source and status;
- a \"Mark Contacted\" row action available only to new leads;
- action permission `leads.edit`;
- delete confirmation;
- count of leads matching current search/filters;
- menu item under CRM.

### Step 1 — Define the resource

```javascript
sync_schema({
  resources: [
    {
      name: \"leads\",
      endpoint: \"/leads\",
      fields: [
        {
          name: \"name\",
          type: \"text\",
          label: \"Name\",
          required: true,
          searchable: true,
          sortable: true
        },
        {
          name: \"email\",
          type: \"email\",
          label: \"Email Address\",
          searchable: true
        },
        {
          name: \"phone\",
          type: \"phone\",
          label: \"Phone #\",
          searchable: true
        },
        {
          name: \"source\",
          type: \"select\",
          label: \"Lead Source\",
          options: {
            site: \"Website\",
            referral: \"Referral\",
            phone: \"Phone Call\",
            trade: \"Trade Show\",
            other: \"Other\"
          },
          default: \"site\",
          required: true,
          filterable: true
        },
        {
          name: \"sourceDescription\",
          type: \"text\",
          label: \"Other Source Description\",
          visibleWhen: \"data.source == 'other'\"
        },
        {
          name: \"status\",
          type: \"select\",
          label: \"Status\",
          options: {
            new: \"New\",
            contacted: \"Contacted\",
            qualified: \"Qualified\",
            lost: \"Lost\"
          },
          default: \"new\",
          required: true,
          filterable: true,
          sortable: true
        },
        {
          name: \"notes\",
          type: \"textarea\",
          label: \"Notes\"
        }
      ],
      actions: [
        {
          permission: \"leads.edit\",
          actionExpression: \"data.status == 'new' ? '/contacted' : null\",
          labelExpression: \"data.status == 'new' ? 'Mark Contacted' : null\",
          confirm: true
        }
      ]
    }
  ]
})
```

Why this is sufficient for the ordinary UI:

- `table`, `form`, and `detail` are omitted because they default to `true`;
- `required` is added only where unconditional requiredness exists;
- `searchable` is explicitly enabled for name/email/phone because its default is `false`;
- `filterable` is explicitly enabled for source/status because its default is `false`;
- default source/status are declarative;
- source description visibility is an expression;
- notes use `textarea`;
- the action is declarative;
- delete confirmation and filtered count will come from `ResourcePage`.

What this resource does **not** enforce by itself:

- email-or-phone cross-field validation;
- source description required only when source is `other`;
- backend state validity for the custom action.

Those need custom code.

---

### Step 2 — Backend validation hook

Both custom validation rules can live in one hook.

```javascript
write_file({
  shell: \"backend\",
  path: \"Resource/Hooks/leads/LeadValidationHook.php\",
  content: `<?php

namespace App\\Resource\\Hooks\\leads;

use App\\Resource\\Hook\\ValidatesResource;

final class LeadValidationHook implements ValidatesResource
{
    public function validate(object $entity, array $data, string $action): array
    {
        $errors = [];

        $email = trim((string) ($entity->email ?? ''));
        $phone = trim((string) ($entity->phone ?? ''));

        if ($email === '' && $phone === '') {
            $message = 'Enter at least an email address or phone number.';
            $errors['email'] = $message;
            $errors['phone'] = $message;
        }

        $source = (string) ($entity->source ?? '');
        $sourceDescription = trim((string) ($entity->sourceDescription ?? ''));

        if ($source === 'other' && $sourceDescription === '') {
            $errors['sourceDescription'] = 'Describe the other lead source.';
        }

        return $errors;
    }
}
`
})
```

Important details:

- whitespace-only email/phone values are treated as empty;
- both email and phone receive the cross-field error, so the form can display the problem clearly;
- `sourceDescription` is checked only when source is `other`;
- backend enforcement does not depend on frontend visibility.

---

### Step 3 — Frontend validation hook

Mirror the backend logic.

```javascript
write_file({
  shell: \"frontend\",
  path: \"resources/hooks/leads/leadValidationHook.ts\",
  content: `import type { ResourceValidationHook } from '../../../types/resource';

const leadValidationHook: ResourceValidationHook = {
  validate: (data, action, record) => {
    const errors: Record<string, string | string[]> = {};

    const email = String(data?.email ?? '').trim();
    const phone = String(data?.phone ?? '').trim();

    if (!email && !phone) {
      const message = 'Enter at least an email address or phone number.';
      errors.email = message;
      errors.phone = message;
    }

    const source = String(data?.source ?? '');
    const sourceDescription = String(data?.sourceDescription ?? '').trim();

    if (source === 'other' && !sourceDescription) {
      errors.sourceDescription = 'Describe the other lead source.';
    }

    return errors;
  },
};

export default leadValidationHook;
`
})
```

Compare the conditions before proceeding:

Backend:

```php
$email === '' && $phone === ''
```

Frontend:

```typescript
!email && !phone
```

Backend:

```php
$source === 'other' && $sourceDescription === ''
```

Frontend:

```typescript
source === 'other' && !sourceDescription
```

They are semantically aligned.

---

### Step 4 — Register frontend validation hook safely

First read:

```javascript
read_file({
  shell: \"frontend\",
  path: \"resources/hooks/index.ts\"
})
```

Assume the current file is:

```typescript
import type { ResourceValidationHook } from '../../types/resource';
import userValidationHook from './users/userValidationHook';

const validationHooks: Record<string, ResourceValidationHook[]> = {
  users: [userValidationHook],
};

export default validationHooks;
```

Write the full updated registry:

```javascript
write_file({
  shell: \"frontend\",
  path: \"resources/hooks/index.ts\",
  content: `import type { ResourceValidationHook } from '../../types/resource';
import userValidationHook from './users/userValidationHook';
import leadValidationHook from './leads/leadValidationHook';

const validationHooks: Record<string, ResourceValidationHook[]> = {
  users: [userValidationHook],
  leads: [leadValidationHook],
};

export default validationHooks;
`
})
```

Do not replace the file with a leads-only registry.

---

### Step 5 — Create backend controller and custom action

```javascript
write_file({
  shell: \"backend\",
  path: \"Controller/LeadController.php\",
  content: `<?php

namespace App\\Controller;

use App\\Entity\\Lead;
use App\\Resource\\ResourceController;
use Symfony\\Component\\HttpFoundation\\JsonResponse;
use Symfony\\Component\\HttpKernel\\Exception\\BadRequestHttpException;
use Symfony\\Component\\HttpKernel\\Exception\\NotFoundHttpException;
use Symfony\\Component\\Routing\\Attribute\\Route;

#[Route('/api/leads', name: 'leads.')]
final class LeadController extends ResourceController
{
    protected function getEntityClass(): string
    {
        return Lead::class;
    }

    protected function getResourceName(): string
    {
        return 'leads';
    }

    #[Route('/{id}/contacted', name: 'contacted', requirements: ['id' => '\\d+'], methods: ['POST'])]
    public function contacted(int $id): JsonResponse
    {
        $this->denyAccessUnlessGranted('leads.edit');

        $record = $this->entityManager
            ->getRepository($this->getEntityClass())
            ->find($id);

        if (!$record) {
            throw new NotFoundHttpException('Record not found');
        }

        if ($record->status !== 'new') {
            throw new BadRequestHttpException('Only new leads can be marked as contacted.');
        }

        $record->status = 'contacted';
        $this->entityManager->flush();

        return $this->json($record);
    }
}
`
})
```

Audit the action against the resource:

Resource permission:

```text
leads.edit
```

Backend permission:

```text
leads.edit
```

Frontend state:

```text
status == new
```

Backend state:

```text
status === new
```

Mutation:

```text
new -> contacted
```

This is consistent.

---

### Step 6 — Create frontend page

```javascript
write_page({
  route: \"/leads\",
  code: `'use client';

import ResourcePage from '@/components/resources/ResourcePage';
import leadsResource from '@/resources/leads';

export default function LeadsPage() {
  return <ResourcePage resource={leadsResource} />;
}
`
})
```

Do not add separate code for:

- source/status filters;
- matching lead count;
- delete confirmation;
- standard forms;
- standard table;
- validation error rendering.

The resource plus `ResourcePage` already provide those behaviors.

---

### Step 7 — Add menu item

```javascript
write_menu({
  name: \"CRM.Leads\",
  route: \"/leads\",
  icon: \"TeamOutlined\",
  after: \"Dashboard\"
})
```

---


## Operating Principles

Use these principles throughout CRUD generation:

1. **Start with `sync_schema`.**
2. **Prefer declarative resource configuration.**
3. **Know the defaults.** `table`, `form`, and `detail` are `true`; `filterable`, `searchable`, `sortable`, `required`, and `unique` are `false`.
4. **Translate requirements literally.** \"Filter by X\" means `filterable: true`; \"search by X\" means `searchable: true`.
5. **Use JavaScript expressions correctly.** Expression properties receive `data` and support ordinary JavaScript expression syntax and core built-ins.
6. **Do not confuse visibility with validation.**
7. **Do not confuse frontend action visibility with authorization or state enforcement.**
8. **Enforce custom validation on both frontend and backend.**
9. **Keep mirrored validation logic semantically identical.**
10. **Read the validation registry before rewriting it.**
11. **Use `ResourceController` for standard backend CRUD.**
12. **Use `ResourcePage` for standard frontend CRUD.**
13. **Rely on built-in delete confirmation and filtered count.**
14. **Keep permissions consistent from resource action to backend route.**
15. **Re-check state on the backend for state-dependent actions.**
16. **Do not hand-build functionality already supplied by the resource system.**
17. **Finish with a requirement-to-implementation audit.**
";

/// Skill for creating a complete CRUD interface across the application stack.
pub struct CrudCreationSkill {
    tags: Vec<String>,
}

impl CrudCreationSkill {
    pub fn new() -> Self {
        Self {
            tags: vec!["crud", "database", "frontend", "backend", "symfony", "react"]
                .into_iter()
                .map(str::to_string)
                .collect(),
        }
    }
}

impl Default for CrudCreationSkill {
    fn default() -> Self { Self::new() }
}

impl Skill for CrudCreationSkill {
    fn name(&self) -> &str { "crud_creation" }
    fn description(&self) -> &str {
        "Teaches the agent how to create a complete CRUD interface with database schema, backend controller, frontend pages, and menu integration."
    }
    fn content(&self) -> &str { CONTENT }
    fn category(&self) -> Option<&str> { Some("development") }
    fn tags(&self) -> &[String] { &self.tags }
}
