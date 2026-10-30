## Dedicated Report Pages & Custom Analytics Views

When a requirement calls for a dedicated report page (e.g. date-range filtered order summaries, detailed audit logs, or custom tabular views beyond the dashboard), create the page using `write_file` (at `app/(dashboard)/<route>/page.tsx` as documented in the system prompt) and use the standalone `DataTable` component.

### `DataTable` Component

The `DataTable` component (`components/crud/DataTable`) provides a table with integrated search, filter inputs (including date range pickers and selects), pagination, and auto-refresh.

#### Props:
- `endpoint`: API endpoint (e.g., `"/analytics/orders/report"`). **Do NOT prefix with `/api`** — the frontend `apiService` automatically prefixes `/api`.
- `columns`: Array of column definition objects (see Column Configuration below).
- `filters`: Array of filter definitions:
  - `{ name: "dateRange", type: "dateRange", label: "Date Range" }`
  - `{ name: "status", type: "select", label: "Status", options: { draft: "Draft", confirmed: "Confirmed" } }`
  - `{ name: "search", type: "text", label: "Search" }`
- `params`: Object of constant query parameters to pass to the endpoint
- `showTotal`: Optional boolean. When `true`, automatically renders a fixed bottom summary row for column totals. (Also auto-enabled if any column specifies `total` or `totalLabel`, or if the API response returns `totals`/`summary`).
- `totalScope`: Optional `'all' | 'page'` (defaults to `'all'`). Controls whether column totals aggregate all fetched records or only the current page.

#### `columns` Configuration & Total Summary Rows:
Each column definition supports:
- `header` / `title` / `label`: Column header title.
- `dataIndex` / `key`: Field key from row records.
- `currency`: String symbol (e.g. `"$"` or `true`). Formats cell values and column totals with the currency symbol and 2 decimal places.
- `format` / `type`: `'currency'` or `'number'` for auto-formatting numbers with thousand separators.
- `decimals`: Custom decimal precision (e.g. `2`).
- `align`: `'left'` | `'center'` | `'right'`. Defaults to `'right'` for numeric/currency columns and `'left'` otherwise.
- `totalLabel`: String (e.g. `"Total"`, `"Total Cost"`, `"BOM Total"`). Overrides cell content in the summary row for this column (typically placed on the first column).
- `total`: Enables column aggregation in the bottom summary row:
  - `true` or `'sum'`: Calculates the sum of numerical values for this column across all records.
  - `'avg'`: Calculates the average value for this column.
  - `(data: any[]) => React.ReactNode`: Custom aggregation function receiving the dataset.
- Server-side totals: If your backend aggregator returns an object containing `{ "items": [...], "totals": { "totalCost": 1250.50 } }`, `DataTable` automatically maps the corresponding `totals[fieldKey]` into the summary row cell and formats it.

### Backend Handler for Dedicated Reports (NEVER Write a Controller!)

> [!CAUTION]
> **DO NOT write a custom Symfony Controller (e.g. `OrderReportController`, `#[Route('/api/analytics/...')]`)**.
> The application has a built-in `AnalyticsController` that routes all `/api/analytics/{category}/{metric}` requests to tagged `AnalyticsAggregatorInterface` services.
> 
> Dedicated reports and data tables MUST be powered by a backend aggregator class in `Service/Analytics/` implementing `AnalyticsAggregatorInterface`.

When `DataTable` fetches data, it sends query parameters:
- `search`: Value from the search input (`?search=...`).
- `filters[<filterName>]`: Scalar filter values (e.g., `?filters[status]=confirmed`).
- `filters[<filterName>][0]` and `filters[<filterName>][1]`: For range filters such as `dateRange` (e.g., start date and end date).
- Any extra parameters passed via the `params` prop.

To access these request parameters in your aggregator, implement `getValueWithRequest(\Symfony\Component\HttpFoundation\Request $request)` (or inspect `Request::createFromGlobals()` in `getValue()`):

#### Step 2: Create the Dedicated Frontend Report Page

Use `write_file` with the `DataTable` component. The `endpoint` property points directly to `"/analytics/orders/report"` (matching `"/analytics/" + aggregator.getName()`):

```javascript
write_file({
  shell: "frontend",
  path: "app/(dashboard)/reports/orders/page.tsx",
  content: `'use client';

import React from 'react';
import { Tag } from 'antd';
import DataTable from '@/components/crud/DataTable';

const statusColors = {
  draft: 'default',
  confirmed: 'blue',
  shipped: 'green',
  cancelled: 'red',
};

export default function OrderReportPage() {
  const columns = [
    { title: 'Order #', dataIndex: 'orderNumber', key: 'orderNumber', totalLabel: 'Total' },
    { title: 'Customer', dataIndex: 'customerName', key: 'customerName' },
    { title: 'Date', dataIndex: 'orderDate', key: 'orderDate' },
    {
      title: 'Status',
      dataIndex: 'status',
      key: 'status',
      render: (status) => <Tag color={statusColors[status] || 'default'}>{status?.toUpperCase()}</Tag>,
    },
    {
      title: 'Total Value',
      dataIndex: 'totalAmount',
      key: 'totalAmount',
      currency: '$',
      total: 'sum',
    },
  ];

  const filters = [
    { name: 'dateRange', type: 'dateRange', label: 'Order Date Range' },
    {
      name: 'status',
      type: 'select',
      label: 'Order Status',
      options: { draft: 'Draft', confirmed: 'Confirmed', shipped: 'Shipped', cancelled: 'Cancelled' },
    },
  ];

  return (
    <div style={{ padding: 24 }}>
      <DataTable
        title="Order & Sales Report"
        endpoint="/analytics/orders/report"
        columns={columns}
        filters={filters}
        showTotal
      />
    </div>
  );
}
`
})
```

#### Step 3: Add to Navigation Menu

Add the report to the navigation menu using `patch_file` on `menu-items/menu.json`:
```javascript
patch_file({
  shell: "frontend",
  patch: `*** Begin Patch
*** Update File: menu-items/menu.json
@@ -15,4 +15,10 @@
     "permission": "roles.index"
   }
+  {
+    "name": "Reports.OrderReport",
+    "route": "/reports/orders",
+    "icon": "BarChartOutlined"
+  }
 ]
*** End Patch`
})
```

---

## Best Practices

1. **Always read before writing dashboard files**: Use `read_file` first on `views/dashboard/default.jsx` so you retain existing widgets inside `<Dashboard>`.
2. **Endpoint matching & 2-segment rule**: The `endpoint` prop in widgets and pages (e.g., `"/analytics/customers/count"`, `"/analytics/products/lowStock"`, `"/analytics/orders/report"`) must match `"/analytics/" + aggregator.getName()`. Aggregator names and endpoints must **strictly follow the 2-segment pattern** (`/analytics/<resource>/<metric>`). Never introduce extra slashes (such as `/analytics/products/lowStock/count`), as this will fail with a 404.
3. **Never create custom Symfony controllers for reports**: Never create an `AbstractController` or `#[Route('/api/analytics/...')]` class. The framework automatically dispatches `/api/analytics/{category}/{metric}` to any service implementing `AnalyticsAggregatorInterface` under `App\Service\Analytics\`. Use `getValueWithRequest(Request $request)` to read filter or search params.
4. **Never include `/api` in frontend endpoints**: Frontend requests go through `apiService` which already prefixes `/api`. Using `"/api/analytics/..."` will result in `"/api/api/analytics/..."` and fail with a 404. Always use `"/analytics/..."`.
5. **Chart data format**: All chart widgets use the documented JSON data formats (Cartesian, Distribution, or Tabular).
6. **Framework-Driven Layout (No `size` prop needed)**: The agent declares content widgets inside `<Dashboard>`, and the framework builds rows automatically:
   - **KPI cards**: One row, equal columns (up to 4 per row, 3 per row beyond that).
   - **Charts** (`LineChartCard`, `BarChartCard`, `PieChartCard`): 2 per row; an odd last chart spans the full width. Chart bodies have a fixed 280px height.
   - **Tables** (`TableCard`): 2 per row; an odd last table spans the full width. Table bodies have a max height and scroll internally.
   - The layout order is always KPIs, then charts, then tables.
   - Every row uses CSS grid with `align-items: stretch` and `height: 100%`, guaranteeing cards in a row match in height.
   - **Do not calculate or pass `size` props** from the agent.
7. **Optimized DB queries & QueryBuilder filtering**:
   - Perform calculations (e.g. `COUNT`, `SUM`, `AVG`) at the database layer via QueryBuilder rather than loading full entity collections into memory.
   - **Always use `andWhere()` (or `orWhere()`) instead of chaining multiple `where()` calls**: In Doctrine QueryBuilder, calling `->where(...)` multiple times overwrites prior conditions. Use `->where(...)` for the initial condition and subsequent `->andWhere(...)` for additional filters.
8. **Entity Associations and Joins**:
   - **No automated ORM navigation associations**: Child entities (such as `OrderItem` to `Order` or `Order` to `OrderItem`), as well as foreign key relationships (such as `Order` to `Customer` via `customerId`), do not have automated ORM navigation associations. Attempting `->innerJoin('o.items', 'i')` or `->innerJoin('i.order', 'o')` will fail with an association mapping error.
   - **Always perform explicit joins with `Join::WITH`**: Specify the entity class and the `Join::WITH` foreign key condition explicitly:
     ```php
     use Doctrine\ORM\Query\Expr\Join;
     use App\Entity\Order;
     use App\Entity\OrderItem;

     // Explicit join: querying from Order to child OrderItem via orderId
     $qb = $this->entityManager->createQueryBuilder()
         ->select("DATE_FORMAT(o.orderDate, '%Y-%m') AS month, SUM(i.quantity * i.unitPrice) AS total")
         ->from(Order::class, 'o')
         ->innerJoin(OrderItem::class, 'i', Join::WITH, 'i.orderId = o.id')
         ->where('o.orderDate >= :startDate')
         ->andWhere('o.status != :cancelled');
     ```
     ```php
     // Explicit join: querying from child OrderItem to parent Order via orderId
     $qb = $this->entityManager->createQueryBuilder()
         ->select("DATE_FORMAT(o.orderDate, '%Y-%m') AS month, SUM(i.quantity * i.unitPrice) AS total")
         ->from(OrderItem::class, 'i')
         ->innerJoin(Order::class, 'o', Join::WITH, 'i.orderId = o.id')
         ->where('o.orderDate >= :startDate')
         ->andWhere('o.status != :cancelled');
     ```
9. **Computed Columns in Aggregators & Reports**:
   - Because computed columns are stored physically in the database and automatically updated on save, you can directly aggregate or select them (e.g. `SUM(o.totalAmount)` or `o.totalAmount AS totalAmount`) without querying child tables or writing correlated subqueries.
   - If you ever need to perform manual joins between parent and child tables (such as grouping by individual items), always use explicit `Join::WITH` syntax (`innerJoin(OrderItem::class, 'i', Join::WITH, 'i.orderId = o.id')`). Do not use unmapped property paths like `'o.items'`.