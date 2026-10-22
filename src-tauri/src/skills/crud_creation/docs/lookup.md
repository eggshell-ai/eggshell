# Lookup Fields

Use `lookup` when selecting a record in a form or table line item should automatically fetch and snapshot data from another resource (e.g. capturing a product's current selling price into an order item at creation time, ensuring future product price changes do not modify historical order values).

Example:

```javascript
{
    fields: [
        ...,
        {
            name: "productId",
            type: "select",
            label: "Product",
            required: true,
            resource: "products"
        },
        {
            name: "unitPrice",
            type: "decimal",
            label: "Price",
            scale: 2,
            required: true,
            lookup: {
                resource: "products",         // Name or endpoint of the resource
                matchValue: "data.productId",  // The value to match with product's ID
                targetField: "price",          // Field to copy from the retrieved record
                on: "change",                  // Triggered when matchValue changes, currently the only option
                overwrite: false               // If false, preserves manual user overrides if already entered
            }
        }
    ]
}
```

Behavior:
- When the user selects a product in the row, `lookup` executes automatically.
- It fetches the target entity and fills `unitPrice` with the current product price.
- Because `unitPrice` is a stored field (not virtual), it persists permanently on the child item.