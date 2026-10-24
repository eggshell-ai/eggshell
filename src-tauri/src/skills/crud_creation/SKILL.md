---
name: crud_creation
description: Teaches the agent how to create a complete CRUD interface with database schema, backend controller, frontend pages, and menu integration.
category: development
tags:
  - crud
  - database
  - frontend
  - backend
  - symfony
  - react
---

# CRUD Creation Skill

This skill explains how to build a CRUD using the resource system. You can build a standard CRUD without writing the entire code from scratch by utilizign the steps below.

Certain features are documented into separate files, load them on demand by using the `load_skill` tool similar to below example:

```javascript
load_skill({ skillName: "crud_creation", document: "hooks_and_lifecycle" })
```

## 1. Create a Resource
A resource is the schema / entity that's used to generate the forms, data grids, and APIs. In order to create a new resource, called `sync_schema` tool

Example
```javascript
sync_schema({
  resources: [
    {
      name: "products",
      endpoint: "/products", # Generally same as the resource name
      titleExpression: "data.sku + ' - ' + data.name", # Display the product's SKU / Name as the readable title of individual products
      actions: [ # Optional: display a "Deactivate" button only if product is active, null hides the button automatically
        {
          permission: "products.edit",
          labelExpression: "data.active ? 'Deactivate' : null",
          confirm: true,
          handler: `
            const result = await api.post('/products/' + data.id + '/toggle', {});
            message.success('Product deactivated successfully');
            await refresh();
          `
        }
      ],
      fields: [
        {
          name: "sku",
          type: "text",
          label: "SKU",
          required: true,
          transforms: [{ type: "trim" }, { type: "uppercase" }] # Uppercase the SKU and remove trailing / leading spaces
        },
        {
          name: "name",
          type: "text",
          label: "Product Name",
          required: true
        },
        {
          name: "price",
          type: "decimal",
          label: "Price"
        },
        {
          name: "active",
          label: "Active",
          type: "boolean",
          default: true,
          displayRules: [
            { condition: "data.active", badge: { text: "Active", variant: "success" } },
            { condition: "!data.active", badge: { text: "Inactive", variant: "error" } }
          ]
        }
      ]
    }
  ]
})
```

### 2.1 Resource properties

A resource object supports the following core properties.

| Property | Required | Default / omission behavior | Meaning |
|---|---:|---|---|
| `name` | yes | none | Resource identifier. Use a stable plural resource name such as `products`, `customers`, or `leads`. |
| `endpoint` | yes for normal CRUD | none | API endpoint such as `/products`. Required for standard CRUD and row actions. |
| `fields` | yes | none | Array of field definitions. |
| `actions` | no | `[]` / no custom actions | Custom row actions shown by the resource grid. |
| `titleExpression` | no | unset | JavaScript expression/template used when a resource-specific title is needed. Specify the title / name field if available |

### Available Field Types
Use one of the following field types:

| Type | Intended use |
|---|---|
| `"text"` | Single-line text input |
| `"textarea"` | Multi-line text input |
| `"email"` | Email input with standard email validation |
| `"phone"` | Phone input with permissive local/international phone validation |
| `"password"` | Password input |
| `"number"` | Numeric integer/float input |
| `"decimal"` | Fixed-point decimal / currency input with configurable `scale` and `precision` |
| `"boolean"` | Boolean / checkbox input |
| `"date"` | Date picker |
| `"file"` | File upload |
| `"select"` | Dropdown; use `options` for static choices or `resource` for dynamic choices |
| `"tags"` | Multi-select tags, normally paired with `resource` |
| `"table"` | Line-item grid, paired with `columns` |

## Field Options
Every field must have `name` and `type`.

These properties are generally available on each type, certain types support additional properties.

| Property | Default | Meaning |
|---|---|---|
| `name` | none; required | Field identifier |
| `type` | none; required | Field type |
| `label` | unset | Display label. Name is title-cased and used if omitted |
| `table` | `true` | Display in list view |
| `form` | `true` | Show in create/edit forms |
| `detail` | `true` | Display when viewing details for a single record |
| `filterable` | `false` | Allow filtering the list view by this field |
| `searchable` | `false` | Allow quick-search in this field (should be true for titles only) |
| `sortable` | `false` | Allow list view filtering by this field |
| `required` | `false` | The value is required |
| `unique` | `false` | Allow only unique values (skips empty values - enable both required / unique to ensure 100% uniqueness) |
| `default` | unset | Default suggestion during new record |
| `minSize` | unset | Min size for text fields |
| `maxSize` | unset | Max size |
| `min` | unset | Minimum for numeric fields |
| `max` | unset | Max. value (similar to above) |
| `transforms` | unset | Pipeline of field transformations (e.g. `[{ type: "trim" }, { type: "uppercase" }]`) applied before validation/persistence |

## Field Options
Every field must have `name` and `type`.

The behavioral defaults are important. Do not assume that omission means `false`.

| Property | Default | Meaning |
|---|---|---|
| `accept` | unset | No field-specific file accept restriction |
| `options` | unset | No static select options |
| `visibleWhen` | unset | Field is not conditionally hidden |
| `columns` | unset | Child table columns for `type: "table"` |
| `map` | unset | Foreign key property name connecting child items to parent (e.g. `"orderId"`) |
| `targetEntity` | unset | Optional override for the child entity class name. Derived automatically from `resource` (e.g. `order_items` -> `OrderItem`), so normally omit it |
| `lookup` | unset | Auto-lookup configuration to snapshot values from a related resource (e.g. `{ resource: "products", matchField: "id", matchValue: "data.productId", targetField: "price" }`) |
| `scale` | `2` (for `decimal`) | Number of decimal places |
| `precision` | `10` (for `decimal`) | Total digits stored |
| `integer` | `false` | Restrict numeric field to whole numbers |
| `computed` | `false` | Virtual / calculated field not stored in the database |
| `computeExpression` | unset | JavaScript expression evaluated against row `data` to compute virtual value |
| `sqlExpression` | unset | SQL/DQL expression for server-side sorting/filtering of virtual fields |
| `displayRules` | unset | Conditional formatting rules (e.g. `[{ condition: "data.quantityInStock === 0", badge: { text: "Out of Stock", variant: "error" } }]`) |

## 2. Create the Frontend Page
Let's consider that we want our product page on "/products", we shall create a page within `app/(dashboard)/products/page.tsx`:

```javascript
write_file({
  shell: "frontend",
  path: "app/(dashboard)/products/page.tsx",
  content: `'use client';

import ResourcePage from '@/components/resources/ResourcePage';
import productsResource from '@/resources/products';

export default function ProductsPage() {
  return <ResourcePage resource={productsResource} />;
}
`
})
```

Resource page will generate a data grid with add, edit / delete buttons, filters and sort where enabled. It will automatically be scoped by `products.create`, `products.edit`, `products.delete` and `products.view` permissions.

Since the default admin user has all permissions, we don't need to worry about that.

## 3. Write the Backend Controller
We shall add our controller at the endpoint used in `sync_schema`.

```php
write_file({
  shell: "backend",
  path: "Controller/ProductController.php",
  content: `<?php

namespace App\Controller;

// The name of our resource, singularized and upper-camel cased
use App\Entity\Product;
use App\Resource\ResourceController;
use Symfony\Component\Routing\Attribute\Route;

// a literal dot must be added after resource name
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
```

## 4. Write the Menu
We must add the menu so the user can access the screen we created, we'll modify `menu.json` to add our new screen

First, we must read the file to verify what we are modifying
```javascript
read_file({
  shell: "frontend",
  patch: "menu-items/menu.json"
})
```
Once we find a suitable location to add our menu, we can use the Open-AI style patch tool, or just rewrite the file entirely

```javascript
patch_file({
  shell: "frontend",
  patch: `*** Begin Patch
*** Update File: menu-items/menu.json
@@
    "permission": "roles.index"
  },
+  {
+    "name": "Inventory.Products",
+    "route": "/products",
+    "icon": "ShoppingOutlined",
+    "permission": "products.view"
+  }
 ]
*** End Patch`
})
```

This will create a complete CRUD where SKU is uppercased and trimmed automatically, product name appears correctly in any future autocompletes, and the user can access the menu from their sidebar.

# Reference Documents
These documents define how to implement different requirements

Use `load_skill` tool to load the files.
Example
```javascript
load_skill({
  skillName: "crud_creation",
  path: "docs/computed.md"
})
```

- `docs/computed.md`: Create computed columns like total amount & full name (from first / last name) with support for sorting and filtering. Also explains how to perform calculation on stored fields before saving (e.g. uppercase SKU automatically)

- `docs/transaction_example.md`: Provides a complete example on how to build financial record CRUDs with multiple associated line items (e.g. vouchers, sales, etc.)

- `docs/validation.md`: Explains custom validation synchronized between backend / frontend, along with customizing messages for validation

- `docs/display.md`: Explains conditional visiblity and custom display (badges, custom labels, etc.) in list view

- `docs/lookup.md`: Load data against a resource (e.g. automatically load price agaisnt a product for UX)

- `docs/editing.md`: Edit an existing resource (e.g. add phone # to existing customer)

- `docs/row_actions.md`: Explains row actions with custom popups and API format (e.g. Add a "Converted" button on each lead, add a quick "Activate / Deactivate" button)

- `hooks_and_lifecycle.md`: Write a custom save method (e.g. through a custom frontend form) or add hooks before and after saving (e.g. update stock after saving order)

---

# Field Examples
Here are some examples of certain types of fields

## Dropdowns

Use `type: "select"` with `options` (for a static list, e.g. select lead status) or `resource` (for dynamic lists, e.g. select customer with order).

### Static Dropdown with Options

The option keys are stored values. The option values are display labels.

```javascript
{
  name: "status",
  type: "select",
  label: "Status",
  options: {
    new: "New",
    contacted: "Contacted",
    qualified: "Qualified",
    lost: "Lost"
  },
  default: "new",
  required: true,
  filterable: true
}
```

### Dynamic Dropdown with Resource

It requires that `products` resource is already created.

```javascript
{
  name: "productId",
  type: "select",
  label: "Product",
  required: true,
  resource: "products"
}
```


## Checkboxes
Checkboxes should have a clear label, both in `label` and during dispaly. By default, they display `Yes` and `No`, which seems unpolished.

Use `displayRules` to define clear badges for both conditions.

```javascript
{
  name: "active",
  label: "Active",
  type: "boolean",
  default: true,
  displayRules: [
    { condition: "data.active", badge: { text: "Active", variant: "success" } },
    { condition: "!data.active", badge: { text: "Inactive", variant: "error" } }
  ]
}
```

Now, in the form, the user will see a checkbox labelled "Active", and in list view, they will see Active / Inactive with color coding.

## Child Tables (One-to-Many relationship)
Child tables provide an unified form to add and remove line items against a parent record (e.g. products against sales, ledger transactions against voucher)

```javascript
{
  name: "items",
  type: "table",
  label: "Items",
  resource: "order_items",
  map: "orderId", // `orderId` must be a field on `order_items` resource
  columns: [
    { name: "productId", label: "Product" },
    { name: "quantity", label: "Quantity" },
    { name: "unitPrice", label: "Unit Price" },
    { name: "subtotal", label: "Subtotal" } // `subtotal` is already `computed: true` on order_items, so it renders as a calculated cell automatically
  ]
}
```

## Operating Principles

Use these principles throughout CRUD generation:

1. Always use permissions and validations
2. Mirror custom validations on backend and frontend
3. For unclear on unsupported requirements, or unexpected errors, notify the user instead of attempting to fix it yourself.