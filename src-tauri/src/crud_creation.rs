use super::{Skill};

// Paste the full CRUD skill instructions in this constant.
const CONTENT: &str = "# CRUD Creation Skill

This skill explains how to create a complete CRUD (Create, Read, Update, Delete) interface in the admin panel system using the declarative resource system with helper components.

## Overview

To create a CRUD interface, you need to coordinate several tools in sequence:

1. **sync_schema** - Define the data model and generate both frontend resource and backend entity
2. **write_controller** - Create the Symfony backend controller extending ResourceController
3. **write_page** - Create React frontend page using ResourcePage component
4. **write_menu** - Add the menu item to navigate to the new pages

## Step-by-Step Process

### Step 1: Define the Schema with sync_schema

Use the `sync_schema` tool to define your data model. This tool generates:
- Frontend resource definition in `frontend/src/resources/{name}.ts`
- Backend PHP entity in `backend/src/Entity/{ClassName}.php`
- Runs database migrations

Example schema definition (pass the pure JavaScript string for `code` without outer backticks):
```javascript
sync_schema({
  code: `defineResource({
  name: \"products\",
  endpoint: \"/products\", // Note: Never include /api here, it's included automatically
  permissions: {
    'view': 'products.view',
    'create': 'products.create',
    'edit': 'products.edit',
    'delete': 'products.delete'
  },
  fields: [
    field.text(\"name\")
      .label(\"Product Name\")
      .table()
      .form()
      .required()
      .searchable(),
    field.text(\"sku\")
      .label(\"SKU\")
      .table()
      .form()
      .required()
      .unique(),
    field.number(\"price\")
      .label(\"Price\")
      .table()
      .form()
      .required(),
    field.file(\"image\")
      .label(\"Product Image\")
      .table()
      .form()
      .accept(\"image/*\")
      .maxSize(2),
    field.textarea(\"description\")
      .label(\"Description\")
      .form(),
    field.boolean(\"active\")
      .label(\"Active\")
      .table()
      .form(),
    field.table(\"items\")
      .label(\"Line Items\")
      .resource(\"invoice_items\")
      .columns(['id', 'product', 'qty', 'rate', { name: 'amount', value: (data) => data.rate * data.qty }])
      .form()
  ]
})`
})
```

### Step 2: Create Backend Controller with write_controller

After the schema is synced, create a Symfony controller extending `ResourceController`. This base class handles all standard CRUD operations automatically.

Example controller:
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

The `ResourceController` base class automatically provides:
- `index()` - List all records
- `show()` - Get a single record
- `store()` - Create a new record
- `update()` - Update an existing record
- `destroy()` - Delete a record

You only need to define:
- `getEntityClass()` - Return the fully qualified entity class name
- `getResourceName()` - Return the plural resource name

Optional: Add `beforeSave()` hook for custom logic (e.g., password hashing).

### Step 3: Create Frontend Page with write_page

Create a single page using the `ResourcePage` component which handles all CRUD operations (list, create, edit, delete) automatically.

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

The `ResourcePage` component automatically provides:
- Data table with sorting and search
- Add/Edit modals with form validation
- Delete confirmation
- Permission-based access control
- Bulk operations

### Step 4: Add Menu Item with write_menu

Add the menu item to make it accessible from the sidebar:
```
write_menu({
  name: \"Inventory.Products\",
  route: \"/products\",
  icon: \"ShoppingOutlined\",
  after: \"Dashboard\"
})
```

## Best Practices

1. **Always start with sync_schema** - This creates the foundation for both frontend and backend
2. **Use consistent naming** - Keep resource names, endpoints, and routes aligned (singular for class, plural for routes)
3. **Define field properties carefully** - Use `table()` for list views, `form()` for forms, `required()` for validation
4. **Use ResourceController** - Don't write manual CRUD methods, extend ResourceController instead
5. **Use ResourcePage** - Don't write custom pages, use the ResourcePage component
6. **Add menu items** - Always add menu items to make new features discoverable
7. **Set permissions** - Define permissions in the resource for access control
8. **Test each step** - Verify each tool call succeeds before proceeding to the next

## Common Patterns

### For simple entities:
- Use basic field types: text, number, boolean, date, textarea
- Extend ResourceController with just the three required methods
- Use ResourcePage for the frontend
- Define basic CRUD permissions

### For entities with relationships:
- Use `field.select(name).resource(resource)` for foreign key relationships
- Use `field.tags(name).resource(resource)` for many-to-many relationships
- Add custom validation in the entity if needed
- Consider adding custom hooks in the controller

### For Line-Item Grids / Tables:
- Use `field.table(name)` to render an ERP-style line item grid inside forms
- Specify target resource using `.resource(res)`
- Define column names and calculated fields using `.columns([...])`:
  ```javascript
  field.table('items')
    .label('Invoice Items')
    .resource('invoice_items')
    .columns(['id', 'product', 'qty', 'rate', { name: 'amount', value: (data) => data.rate * data.qty }])
    .form()
  ```
- Automatically maintains `id` as a hidden field and supports dynamic auto-calculated fields (e.g., `amount: rate * qty`).

### For file uploads:
- Use `field.file(name)` with `.accept(\"image/*\")` and `.maxSize(2)` (size in MB)
- Show in form using `.form()` and optionally in tables using `.table()`

### For complex entities:
- Add `beforeSave()` hook in controller for custom logic (password hashing, timestamps)
- Add custom validation attributes in the entity
- Override serialization groups if needed
- Add additional permissions for specific actions

## Tool Reference

- **sync_schema**: Generates schema, entity, and runs migrations
- **write_controller**: Creates Symfony backend controllers (extend ResourceController)
- **write_page**: Creates React page components (use ResourcePage)
- **write_menu**: Adds/updates menu items in the navigation

## Available Field Types

- `field.text(name)` - Text input
- `field.textarea(name)` - Multi-line text input
- `field.email(name)` - Email input with validation
- `field.password(name)` - Password input
- `field.number(name)` - Numeric input
- `field.boolean(name)` - Boolean/checkbox input
- `field.date(name)` - Date picker
- `field.file(name)` - File upload input
- `field.select(name).resource(resource)` - Dropdown with remote data source
- `field.tags(name).resource(resource)` - Multi-select tags with remote data source
- `field.table(name)` - Line-item grid table widget for ERP-style line items

## Field Chainable Methods

- `.label(\"Display Name\")` - Set the field label
- `.table()` - Show in data table
- `.form()` - Show in create/edit form
- `.required()` - Mark as required
- `.unique()` - Add unique constraint
- `.searchable()` - Enable search in table
- `.sortable()` - Enable sorting in table
- `.accept(\"image/*\")` - Restrict file upload types
- `.maxSize(2)` - Set maximum file upload size in MB
- `.resource(resource)` - Set target resource (object or name string)
- `.columns([...])` - Define column names and calculated fields for table widgets";

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
