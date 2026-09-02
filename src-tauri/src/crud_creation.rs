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
* `messages: {...}` - Optional custom validation messages
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

Boolean filters should provide All / Active / Inactive behavior.

Search and filters should work together.

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
* `\"select\"` - Remote dropdown (pair with `resource`)
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
* `columns: [...]`

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
