# Computed (Virtual) Fields

Use `computed: true` with `computeExpression` when a value is derived from other fields (such as `unitPrice * quantityInStock`) and should not be physically stored in the database.

Example:

```javascript
{
  name: "inventoryValue",
  type: "decimal",
  label: "Inventory Value",
  scale: 2,
  computed: true,
  computeExpression: "(Number(data.unitPrice || 0) * Number(data.quantityInStock || 0)).toFixed(2)",
  sqlExpression: "e.unitPrice * e.quantityInStock",
  table: true,
  detail: true,
  sortable: true
}
```

Behavior:
- `computed: true` ensures no ORM database column is generated;
- `computeExpression` calculates the value dynamically in tables and detail views;
- `sqlExpression` (optional) allows server-side sorting or filtering if needed.
- In the form, computed values are `readonly` and user won't be able to enter them manually

It is also possible to perform computation over child tables.

The child resource owns the calculation: declare `computed: true` + `computeExpression` on the child field (e.g. `subtotal` on `order_items`). The parent table field then only lists the column name — the line-item editor detects the computed child field and renders a read-only calculated cell, storing the value on the child item. **Do not** add `isCalculated: true` in the parent `columns`; it is only needed for ad-hoc columns that have no backing child field (e.g. an inline `value` function).

```javascript
{
  fields: [
    ...,
    {
      name: "items",
      type: "table",
      label: "Order Items",
      resource: "order_items",
      map: "orderId",
      columns: [
        { name: "productId", label: "Product" },
        { name: "quantity", label: "Quantity" },
        { name: "unitPrice", label: "Unit Price" },
        // `subtotal` is `computed: true` on `order_items`, so this column is
        // rendered read-only and calculated automatically. Do not add
        // `isCalculated: true` here.
        { name: "subtotal", label: "Subtotal" }
      ]
    },
    {
      name: "totalAmount",
      type: "decimal",
      label: "Total Amount",
      scale: 2, // Store up to 2 decimal points
      computed: true,
      computeExpression: "Array.isArray(data.items) ? data.items.reduce((sum, item) => sum + (Number(item.quantity || 0) * Number(item.unitPrice || 0)), 0).toFixed(2) : '0.00'",
      sqlExpression: "(SELECT COALESCE(SUM(i.quantity * i.unitPrice), 0) FROM App\\Entity\\OrderItem i WHERE i.orderId = e.id)", // Use App\\Entity\\<EntityName>. The resource name will be converted being singularized and upper-camel-cased (e.g. order_items -> OrderItem)
      form: true,
      table: true,
      sortable: true
    }
  ]
}
```


# Transforms

Use `transforms` to sanitize or format field values before validation and persistence.

Supported built-in transformers include:
- `{ type: "uppercase" }` (or `"upper"`)
- `{ type: "lowercase" }` (or `"lower"`)
- `{ type: "trim" }`
- `{ type: "slugify" }`
- `{ type: "custom", expression: "..." }`

Example:

```javascript
{
  name: "sku",
  type: "text",
  label: "SKU",
  required: true,
  unique: true,
  transforms: [
    { type: "trim" },
    { type: "uppercase" }
  ]
}
```

This transforms `" prod-123 "` into `"PROD-123"` automatically upon form submission.