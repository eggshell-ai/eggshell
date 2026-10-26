# Computed Fields (Stored & Automatically Evaluated)

Use `computed: true` with `computeExpression` and `sqlExpression` when a value is derived from other fields (such as `unitPrice * quantityInStock` or a child line rollup).

Computed fields are **physically stored in the database** as ORM columns and are automatically evaluated and saved by `ResourceController` after every `store` and `update` action. Because they are stored columns, you can freely query, filter, sort, and aggregate them in Doctrine ORM / DQL (for example, `SELECT COALESCE(SUM(s.totalAmount), 0)` in analytics aggregators).

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
- `computed: true` generates a typed database column with `#[ORM\Column(..., nullable: true)]` and a `#[Computed(sqlExpression: '...')]` attribute on the PHP entity.
- The backend `ResourceController` automatically executes the `sqlExpression` after flushing the entity (child line items are computed first, followed by the parent record) and persists the computed value.
- `computeExpression` calculates the value client-side in forms, tables, and detail views for instantaneous UX feedback.
- In the form, computed fields are read-only so users cannot manually overwrite them.
- In analytics aggregators, you can directly reference computed columns (e.g. `SUM(s.totalAmount)`) without complex correlated subqueries or ad-hoc recalculations.

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

---

# Auto-Increment / Serial Sequences (SI Numbers)

For sequence numbers or serial codes (such as `1, 2, 3, ...` or date/month-wise formats like `20261001, 20261002, ...`), use a standard field definition in the resource schema paired with an auto-discovered `BeforeSavesResource` lifecycle hook.

This approach keeps the declarative schema clean (without inventing custom attributes) and ensures sequence numbers are assigned safely on initial record creation (`$action === 'store'`), preventing values from inadvertently shifting during subsequent updates.

### 1. Resource Field Schema

Define the field without manual input on the form:

```javascript
{
  name: "siNumber",
  type: "text", // Use "number" with integer: true for pure numeric counters
  label: "SI #",
  form: false,  // Generated automatically on save; hidden from the create/edit form
  table: true,
  detail: true,
  sortable: true,
  searchable: true,
  unique: true
}
```

### 2. Auto-Discovered Hook (`Resource/Hooks/<resourceName>/<ClassName>.php`)

Implement `App\Resource\Hook\BeforeSavesResource`. The hook generates and assigns the sequence before the entity is validated and persisted.

#### Example A: Simple Sequential Auto-Increment (`1, 2, 3, ...`)

```php
<?php

namespace App\Resource\Hooks\invoices;

use App\Entity\Invoice;
use App\Resource\Hook\BeforeSavesResource;
use Doctrine\ORM\EntityManagerInterface;

final class InvoiceSequenceHook implements BeforeSavesResource
{
    public function beforeSave(
        object $entity,
        array $data,
        string $action,
        EntityManagerInterface $entityManager
    ): void {
        // Only generate for new records if not already set
        if ($action !== 'store' || !empty($entity->siNumber)) {
            return;
        }

        $max = (int) $entityManager
            ->createQuery('SELECT COALESCE(MAX(e.siNumber), 0) FROM ' . Invoice::class . ' e')
            ->getSingleScalarResult();

        $entity->siNumber = $max + 1;
    }
}
```

#### Example B: Month-Wise Auto-Increment (`20261001, 20261002, ...`)

Generates serial numbers scoped by month (e.g. `YYYYMM` prefix followed by a zero-padded incremental sequence):

```php
<?php

namespace App\Resource\Hooks\invoices;

use App\Entity\Invoice;
use App\Resource\Hook\BeforeSavesResource;
use Doctrine\ORM\EntityManagerInterface;

final class InvoiceSequenceHook implements BeforeSavesResource
{
    public function beforeSave(
        object $entity,
        array $data,
        string $action,
        EntityManagerInterface $entityManager
    ): void {
        if ($action !== 'store' || !empty($entity->siNumber)) {
            return;
        }

        // Derive YYYYMM prefix from the record date or current date
        $date = $entity->invoiceDate instanceof \DateTimeInterface
            ? $entity->invoiceDate
            : (isset($entity->invoiceDate) ? new \DateTimeImmutable($entity->invoiceDate) : new \DateTimeImmutable());
        $prefix = $date->format('Ym'); // e.g. "202610"

        $max = $entityManager->createQuery(
            'SELECT MAX(e.siNumber) FROM ' . Invoice::class . ' e WHERE e.siNumber LIKE :prefix'
        )
            ->setParameter('prefix', $prefix . '%')
            ->getSingleScalarResult();

        $nextSeq = $max ? ((int) substr((string) $max, strlen($prefix))) + 1 : 1;

        // Formats as 20261001, 20261002, etc.
        $entity->siNumber = sprintf('%s%02d', $prefix, $nextSeq);
    }
}
```