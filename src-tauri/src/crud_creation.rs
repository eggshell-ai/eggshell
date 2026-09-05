use super::{Skill};

// Paste the full CRUD skill instructions in this constant.
const CONTENT: &str = "# CRUD Creation Skill

This skill explains how to create a complete CRUD (Create, Read, Update, Delete) interface in the admin panel system using the declarative resource system with helper components.

## Overview

To create a CRUD interface, coordinate these tools in sequence:

1. **sync_schema** - Define the data model and generate frontend resource, backend entity, validation, and migrations
2. **write_controller** - Create the Symfony backend controller extending ResourceController
3. **write_page** - Create React frontend page using ResourcePage
4. **write_menu** - Add the menu item

Prefer declarative resource configuration over custom frontend/backend code whenever possible.

## Step 1: Define the Schema with sync_schema

`sync_schema` accepts structured JSON resource definitions via the `resources` parameter (an array of resource objects). Each resource object has a `name`, an `endpoint`, and a `fields` array. Each field object has a `name` and a `type`, plus optional declarative options.

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
          table: true,
          form: true,
          required: true,
          searchable: true
        },
        {
          name: \"sku\",
          type: \"text\",
          label: \"SKU\",
          table: true,
          form: true,
          required: true,
          unique: true
        },
        {
          name: \"price\",
          type: \"number\",
          label: \"Price\",
          table: true,
          form: true,
          required: true
        },
        {
          name: \"active\",
          type: \"boolean\",
          label: \"Status\",
          table: true,
          form: true
        },
        {
          name: \"image\",
          type: \"file\",
          label: \"Product Image\",
          table: true,
          form: true,
          accept: \"image/*\",
          maxSize: 2
        }
      ],
      titleExpression: \"{name}\"
    }
  ]
})
```

An optional `projectPath` string may also be provided to target a specific project.

`sync_schema` should generate:

* Frontend resource definition
* Backend entity
* Database migrations
* Backend validation corresponding to declarative field rules

Validation must be enforced on the backend as well as the frontend.

## Validation

Use field options for ordinary validation:

```javascript
{
  name: \"fullName\",
  type: \"text\",
  required: true,
  minSize: 2
}

{
  name: \"email\",
  type: \"email\",
  required: true,
  unique: true
}

{
  name: \"phone\",
  type: \"phone\"
}
```

Supported validation behavior:

* `required: true` - Value is required
* `unique: true` - Value must be unique; enforce in backend/database
* `type: \"email\"` - Validate email format
* `type: \"phone\"` - Accept reasonable local/international phone formats
* `messages: {...}` - Optional custom validation messages. In most cases, you should skip this unless explicitly asked. \"Helpful validation messages\" are automatically provided by the app.
* `minSize: n` - optional minimum value length
* `maxSize: n` - Optional maximum value length

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

Backend validation errors should be returned per field so ResourcePage can display them beside the corresponding input.

## Static Selects

Use `type: \"select\"` with `options` for a static dropdown. The keys are the stored values while the values are the display labels:

```javascript
{
  name: \"status\",
  type: \"select\",
  label: \"Status\",
  options: {
    \"New\": \"New\",
    \"Old\": \"Old\"
  },
  table: true,
  form: true,
  required: true
}
```

The value is validated to be one of the option keys (or empty when not required) on both frontend and backend.

For dynamic dropdowns backed by another resource, use `type: \"select\"` with `resource` instead.

## Defaults

Use `default` for new-record defaults:

```javascript
{
  name: \"active\",
  type: \"boolean\",
  default: true
}
```

Defaults apply when creating records and must not overwrite values during editing.

## Search

Use `searchable: true` to include a field in standard resource search:

```javascript
{ name: \"fullName\", type: \"text\", searchable: true }
{ name: \"email\", type: \"email\", searchable: true }
```

Search should operate server-side and match any searchable field.

## Filtering

Use `filterable: true` to generate standard filters:

```javascript
{
  name: \"active\",
  type: \"boolean\",
  trueLabel: \"Active\",
  falseLabel: \"Inactive\",
  filterable: true
}
```

While search allow a single field to fuzzy-search within multiple text fields, filters provide explicit control and easier searching in boolean fields.

## Detail/View Pages

Use `detail: true` to show a field in the standard record view:

```javascript
{
  name: \"fullName\",
  type: \"text\",
  table: true,
  detail: true,
  form: true
}
```

If detail fields are defined, ResourcePage should provide a standard View action automatically.

## Boolean Labels

Use:

```javascript
{
  name: \"active\",
  type: \"boolean\",
  trueLabel: \"Active\",
  falseLabel: \"Inactive\"
}
```

to display domain-specific labels while storing boolean values.

These labels should be used consistently in tables, forms, filters, and detail views.

## Step 2: Create Backend Controller with write_controller

Create a Symfony controller extending `ResourceController`.

```php
write_controller({
  controllerName: \"Product\",
  code: `<?php

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

`ResourceController` automatically provides:

* List
* View
* Create
* Update
* Delete
* Search
* Filtering
* Sorting
* Validation handling

Use custom controller code only for behavior that cannot be expressed declaratively.

## Step 3: Create Frontend Page with write_page

Use `ResourcePage` for standard CRUD interfaces.

```tsx
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

ResourcePage automatically provides:

* Data table
* Search
* Filters
* Add/Edit forms
* View action
* Validation messages
* Delete confirmation
* Permission-based access control
* Bulk operations

Do not build custom forms or tables when ResourcePage supports the required behavior.

## Step 4: Add Menu Item with write_menu

```javascript
write_menu({
  name: \"Inventory.Products\",
  route: \"/products\",
  icon: \"ShoppingOutlined\",
  after: \"Dashboard\"
})
```

## Example: Customer Management

```javascript
sync_schema({
  resources: [
    {
      name: \"customers\",
      endpoint: \"/customers\",
      fields: [
        {
          name: \"fullName\",
          type: \"text\",
          label: \"Full Name\",
          table: true,
          detail: true,
          form: true,
          required: true,
          minSize: 2,
          searchable: true
        },
        {
          name: \"email\",
          type: \"email\",
          label: \"Email Address\",
          table: true,
          detail: true,
          form: true,
          required: true,
          unique: true,
          searchable: true
        },
        {
          name: \"phone\",
          type: \"phone\",
          label: \"Phone Number\",
          table: true,
          detail: true,
          form: true
        },
        {
          name: \"companyName\",
          type: \"text\",
          label: \"Company Name\",
          table: true,
          detail: true,
          form: true
        },
        {
          name: \"active\",
          type: \"boolean\",
          label: \"Status\",
          table: true,
          detail: true,
          form: true,
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

This should require no custom controller or page logic beyond the standard ResourceController and ResourcePage.

## Best Practices

1. **Always start with sync_schema**
2. **Prefer declarative resource configuration over custom code**
3. **Enforce validation on both frontend and backend**
4. **Use database constraints for uniqueness**
5. **Use `searchable` and `filterable` instead of custom query code**
6. **Use `default` instead of hardcoding form defaults**
7. **Use `detail` for standard record views**
8. **Use ResourceController for standard CRUD**
9. **Use ResourcePage for standard frontend CRUD**
10. **Keep naming consistent across resource, route, controller, and permissions**
11. **Test each tool call before proceeding**

## Available Field Types

Use these as the `type` value of a field object:

* `\"text\"` - Text input
* `\"textarea\"` - Multi-line text input
* `\"email\"` - Email input with validation
* `\"phone\"` - Phone input with permissive phone validation
* `\"password\"` - Password input
* `\"number\"` - Numeric input
* `\"boolean\"` - Boolean/checkbox input
* `\"date\"` - Date picker
* `\"file\"` - File upload input
* `\"select\"` - Dropdown. Use with `options` for static choices, or pair with `resource` for remote data
* `\"tags\"` - Multi-select tags (pair with `resource`)
* `\"table\"` - Line-item grid (pair with `columns`)

## Field Options

Each field object supports:

* `name` (required) - Field identifier
* `type` (required) - One of the field types above
* `label: \"Display Name\"`
* `table: true`
* `detail: true`
* `form: true`
* `required: true`
* `unique: true`
* `default: value`
* `searchable: true`
* `sortable: true`
* `filterable: true`
* `messages: {...}`
* `trueLabel: \"Label\"`
* `falseLabel: \"Label\"`
* `accept: \"image/*\"`
* `minSize: 2`
* `maxSize: 2`
* `resource: {...}`
* `options: { \"value\": \"Label\", ... }` - Static select choices (keys are values, values are labels)
* `columns: [...]`

## Custom Validators

For validation that cannot be expressed declaratively (e.g. cross-field rules, checks against external data), use validation hooks.

Custom validators must only be created via hooks. Do not add custom validation code to controllers or components; ResourcePage already displays validation errors per field.

Validation hooks exist on both the backend and the frontend, and both should be created together so the user gets immediate feedback in the form while the backend still enforces the rule authoritatively.

### Backend Validation Hooks

Create a hook class under `src/Resource/Hooks/<resourceName>/`. The class name must exactly match the file name, and the namespace must be `App\\Resource\\Hooks\\<resourceName>`. Implement `ValidatesResource` and return an array of `field => message(s)` errors (empty array when validation passes):

Example:

write_file({
  shell: \"backend\",
  path: \"Resource/Hooks/invoices/InvoiceValidationHook.php\",
  content: `<?php

namespace App\\Resource\\Hooks\\invoices;

use App\\Resource\\Hook\\ValidatesResource;

class InvoiceValidationHook implements ValidatesResource
{
    public function validate(object $entity, array $data, string $action): array
    {
        $errors = [];

        if (($entity->total ?? 0) < 0) {
            $errors['total'] = 'Total cannot be negative.';
        }

        if (($entity->discount ?? 0) > ($entity->total ?? 0)) {
            $errors['discount'] = 'Discount cannot exceed the total.';
        }

        return $errors;
    }
}
`})

The hook runs automatically during `store` and `update` after standard declarative validation. Returned errors produce a `422` response in the same format as standard validation errors, so ResourcePage displays them beside the corresponding inputs.

A hook class may implement multiple hook interfaces to participate in additional lifecycle points as they become available.

### Frontend Validation Hooks

Mirror each backend validation hook with a frontend hook under `src/resources/hooks/<resourceName>/`. The hook implements `ResourceValidationHook` and returns a map of `field => message(s)` errors (empty object when validation passes). It receives `(data, action, record)` where `action` is `'store'` or `'update'` and `record` is the existing record on update.

Frontend hooks run automatically during form validation and on submit; their errors are displayed beside the corresponding inputs, exactly like backend errors.

Example:

write_file({
  shell: \"frontend\",
  path: \"resources/hooks/invoices/invoiceValidationHook.ts\",
  content: `import type { ResourceValidationHook } from '../../../types/resource';

const invoiceValidationHook: ResourceValidationHook = {
  validate: (data, action, record) => {
    const errors: Record<string, string | string[]> = {};

    if ((data?.total ?? 0) < 0) {
      errors.total = 'Total cannot be negative.';
    }

    if ((data?.discount ?? 0) > (data?.total ?? 0)) {
      errors.discount = 'Discount cannot exceed the total.';
    }

    return errors;
  },
};

export default invoiceValidationHook;
`
})

After creating a frontend hook file, it must be registered in `src/resources/hooks/index.ts`. This registry file contains only the hook imports and the registry map — no other code — so it can be cheaply read and overwritten.

To register a hook, first read the registry with `read_file`, then overwrite it entirely with the full updated content via `write_file`:

1. `read_file({ shell: \"frontend\", path: \"resources/hooks/index.ts\" })`
2. `write_file({ shell: \"frontend\", path: \"resources/hooks/index.ts\", content: <full updated registry> })`

The registry looks like this:

```javascript
import type { ResourceValidationHook } from '../../types/resource';
import userValidationHook from './users/userValidationHook';
import invoiceValidationHook from './invoices/invoiceValidationHook';

const validationHooks: Record<string, ResourceValidationHook[]> = {
  users: [userValidationHook],
  invoices: [invoiceValidationHook]
};

export default validationHooks;
```

### Modifying an Existing Hook

When a validation hook already exists and needs to be changed, overwrite the hook file entirely with `write_file` using the full updated content. Never append to or partially patch a hook file.

Example workflow:

1. `write_file({ shell: \"backend\", path: \"Resource/Hooks/invoices/InvoiceValidationHook.php\", content: <full updated file> })`
2. `write_file({ shell: \"frontend\", path: \"resources/hooks/invoices/invoiceValidationHook.ts\", content: <full updated file> })`

Remember to keep the backend hook and its frontend mirror in sync: any rule added to one should be reflected in the other.

## Custom Code

Use custom controller hooks or custom pages only for behavior such as:

* Complex cross-field validation
* Workflow/state transitions
* External API calls
* Multi-record operations
* Specialized UI behavior

Ordinary validation, defaults, search, filtering, detail views, uniqueness, and delete confirmation should remain declarative.
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
