# Dashboard Widgets
This document defines how to create a dashboard with KPI cards, charts and tables. This is the first thing the user is going to see once they open the portal.

## Workflow

1. **Create The Backend Aggregator**
```javascript
write_file({
  shell: "backend",
  path: "Service/Analytics/TotalRevenueAggregator.php",
  content: `<?php

namespace App\Service\Analytics;

use App\Entity\Order;
use Doctrine\ORM\EntityManagerInterface;

class TotalRevenueAggregator implements AnalyticsAggregatorInterface
{
    public function __construct(
        private readonly EntityManagerInterface $entityManager
    ) {}

    public function getName(): string
    {
        return 'sales/revenue';
    }

    public function getValue(): int
    {
        return (int) $this->entityManager
            ->getRepository(Order::class)
            ->createQueryBuilder('o')
            ->select('SUM(o.totalAmount)')
            ->getQuery()
            ->getSingleScalarResult();
    }
}
`
})
```

The return value depends upon the type of widget you will use.

- **KPI Cards**: Return a single number or string
- **Charts / Tables**: Expects different formats depending upon the chart type

#### Format 1: Cartesian Charts (`LineChartCard`, `BarChartCard`)
Expects the backend to return JSON data in the following format (multiple series supported):

```json
{
  "xAxis": ["Jan", "Feb", "Mar", "Apr", "May", "Jun"],
  "series": [
    {
      "data": [10, 25, 30, 45, 40, 55],
      "label": "Sales"
    }
  ]
}
```

#### Format 2: Breakdown / Distribution Charts (`PieChartCard`)
Expects an array of objects or an object containing `series`:

```json
[
  { "id": 1, "label": "Draft", "value": 12 },
  { "id": 2, "label": "Confirmed", "value": 45 },
  { "id": 3, "label": "Shipped", "value": 30 },
  { "id": 4, "label": "Cancelled", "value": 3 }
]
```
Or:
```json
{
  "series": [
    { "label": "Draft", "value": 12 },
    { "label": "Confirmed", "value": 45 }
  ]
}
```

#### Format 3: Tabular Reports (`TableCard`)
Expects an array of row objects (or `{ "data": [...] }` or `{ "items": [...] }`):

```json
[
  { "id": 1, "name": "Widget A", "sku": "WID-A", "totalSold": 150, "totalRevenue": 2999.50 },
  { "id": 2, "name": "Gadget B", "sku": "GAD-B", "totalSold": 95, "totalRevenue": 1425.00 }
]
```

2. **Read the dashboard file**:
Before overwriting, read the existing file

```javascript
read_file({
  shell: "frontend",
  path: "views/dashboard/default.jsx"
})
```

3. **Add the new Widgets**: Use `patch_file` to add the new widgets in the preferred position:

```javascript
patch_file({
  shell: "frontend",
  path: "views/dashboard/default.jsx",
  patch: `@@
-    <Dashboard title="Dashboard">
+    <Dashboard title="Dashboard">
+      <KPICard
+        title="Total Revenue"
+        endpoint="/analytics/revenue/total"
+        currency="$"
+      />
 `
})
```

Also add the required import
```javascript
patch_file({
  shell: "frontend",
  path: "views/dashboard/default.jsx",
  patch: `@@
-// project imports
+// project imports
+import KPICard from 'components/cards/statistics/KPICard';
 `
})
```

## Chart Widgets

The following chart and reporting widgets are available for displaying analytics data:

- **LineChartCard** (`components/cards/statistics/LineChart`): Displays trend data as a line chart with support for multiple series.
- **BarChartCard** (`components/cards/statistics/BarChart`): Displays categorical or time-bucket comparisons as a bar chart.
- **PieChartCard** (`components/cards/statistics/PieChart`): Displays distribution/breakdown data (e.g. sales by order status, customer tiers). Pass `innerRadius` (e.g. `innerRadius={40}`) for donut charts.
- **TableCard** (`components/cards/statistics/TableCard`): Displays ranked / top-N tabular data (e.g. top-selling products, low-stock items) with clickable resource link templates (e.g. `link: "/products/{id}"`).

### Tabkes

Tables support a `columns` attribute to customize their display

```jsx
<TableCard
  title="Top Selling Products"
  endpoint="/products/topSelling"
  maxRows={5}
  columns={[
    {
      header: 'Product',
      dataIndex: 'name',
      link: '/products/{id}', // backend must return an `id` field
    },
    {
      header: 'Units Sold',
      dataIndex: 'totalSold',
      format: 'number',
    },
    {
      header: 'Revenue',
      dataIndex: 'totalRevenue',
      currency: '$',
    },
    { // example column demonstrating all options
        header: 'Example',
        dataIndex: 'example',
        align: 'center', // "left", "right" or "center". numbers and currencies are right-aligned by default
        currency: "EUR",
        compact: false,
        decimals: 0, // number of decimals
        link: '/customers
    }
  ]}
/>
```

### KPICards

- `currency`: Currency symbol or code (e.g., `"$"` or `"USD"`). When provided, values are automatically formatted with the currency symbol and localized styling (the same parameter is supported on all other charts).
- `compact`: Optional boolean flag controlling number compaction.
  - When `currency` is passed and `compact` is omitted, compaction **defaults to `true`**, formatting large figures with abbreviated suffixes (e.g. `$14.82m` instead of `$14,820,000.00`).
  - Pass `compact={false}` if you want the full uncompacted value with thousand separators (e.g. `$14,820,000.00`).