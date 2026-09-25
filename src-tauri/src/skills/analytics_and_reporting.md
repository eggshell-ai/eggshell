# Analytics and Reporting Skill

This skill explains how to build analytics and reporting capabilities in the system. It covers creating backend analytics aggregator services and adding widgets to the frontend dashboard.

## Overview

Creating analytics metrics and reports (both dashboard widgets and dedicated report pages) involves two primary parts:

1. **Backend Aggregator / Handler**: A PHP service class in `Service/Analytics/` implementing `AnalyticsAggregatorInterface` that queries the database using Doctrine ORM.
   - **NEVER create a custom Symfony Controller (e.g., `OrderReportController`, `#[Route('/api/analytics/...')]`)**.
   - All analytics endpoints (`/api/analytics/{category}/{metric}`) are centrally routed and handled by the system's `AnalyticsController`, which automatically autowires and dispatches to services implementing `AnalyticsAggregatorInterface`.
   - Dedicated reports and datatables use the exact same aggregator mechanism as dashboard widgets.
2. **Frontend View**: Either updating `views/dashboard/default.jsx` with KPI/chart widgets inside `<Dashboard>` or creating a dedicated report page with `DataTable`.

To read and write these files across shells, use the `read_file` and `write_file` tools.

---

## Tooling & File Access

Use the `read_file` and `write_file` tools to interact with frontend and backend files:

- **`read_file`**:
  - `shell`: `"frontend"` or `"backend"`
  - `path`: Relative path inside `src` directory (e.g., `views/dashboard/default.jsx` or `Service/Analytics/CustomerCountAggregator.php`)
- **`write_file`**:
  - `shell`: `"frontend"` or `"backend"`
  - `path`: Relative path inside `src` directory
  - `content`: The complete updated content to write

---

## Entity Naming Conventions

When querying the database in backend aggregators:
- Entities live under the `App\Entity` namespace.
- Entity class names are the **singular**, **UpperCamelCase** version of the resource name.

### Examples:
- Resource `customers` &rarr; `App\Entity\Customer`
- Resource `orders` &rarr; `App\Entity\Order`
- Resource `invoices` &rarr; `App\Entity\Invoice`
- Resource `product_categories` &rarr; `App\Entity\ProductCategory`

---

## Step-by-Step Implementation

### Step 1: Create Backend Analytics Aggregator

Create a new aggregator service in the backend under `Service/Analytics/` using `write_file`.

**Tool call example:**
```javascript
write_file({
  shell: "backend",
  path: "Service/Analytics/CustomerCountAggregator.php",
  content: `<?php

namespace App\Service\Analytics;

use App\Entity\Customer;
use Doctrine\ORM\EntityManagerInterface;

class CustomerCountAggregator implements AnalyticsAggregatorInterface
{
    public function __construct(
        private readonly EntityManagerInterface $entityManager
    ) {}

    public function getName(): string
    {
        return 'customers/count';
    }

    public function getValue(): int
    {
        return (int) $this->entityManager
            ->getRepository(Customer::class)
            ->createQueryBuilder('c')
            ->select('COUNT(c.id)')
            ->getQuery()
            ->getSingleScalarResult();
    }
}
`
})
```

#### Key Aggregator Requirements:
- Must implement `App\Service\Analytics\AnalyticsAggregatorInterface`.
- `getName()`: Returns the unique metric key path corresponding to `/analytics/{getName()}`.
  - **CRITICAL: Format must strictly consist of exactly TWO segments**: `<resource>/<metric>` (e.g., `'customers/count'`, `'products/lowStock'`).
  - **Never use 3 or more segments with slashes**: e.g., `'products/lowStock/count'` ❌ will cause a **404 Not Found** because the backend analytics routing matches `/analytics/{resource}/{metric}`.
  - For specific or compound metrics, use camelCase for the second segment instead (e.g., `'products/lowStock'` or `'products/lowStockCount'` ✅).
- `getValue()`: Calculates and returns the aggregate metric value (e.g., total count, sum, average, or array of objects).

---

### Step 2: Read Frontend Dashboard

Before modifying the dashboard, read `views/dashboard/default.jsx` using `read_file` to see existing widgets and preserve their structure.

**Tool call example:**
```javascript
read_file({
  shell: "frontend",
  path: "views/dashboard/default.jsx"
})
```

---

### Step 3: Add Widget to Dashboard

Update `views/dashboard/default.jsx` using `write_file` to add the new `KPICard` widget inside `<Dashboard>`.

The `<Dashboard>` container automatically handles title headers, refresh actions, and automated cache registration/invalidation. Widgets declare their grid size via the `size` prop (using a 12-column grid system, e.g. `size={{ xs: 12, sm: 6, lg: 3 }}` or `size={3}`).

**Tool call example:**
```javascript
write_file({
  shell: "frontend",
  path: "views/dashboard/default.jsx",
  content: `'use client';

// project imports
import Dashboard from 'components/dashboard/Dashboard';
import KPICard from 'components/cards/statistics/KPICard';

// ==============================|| DASHBOARD - DEFAULT ||============================== //

export default function DashboardDefault() {
  return (
    <Dashboard title="Dashboard">
      <KPICard
        title="Customers"
        endpoint="/analytics/customers/count"
        size={{ xs: 12, sm: 6, lg: 3 }}
      />
    </Dashboard>
  );
}
`
})
```

---

## Chart Widgets

### Available Chart Types

The following chart and reporting widgets are available for displaying analytics data:

- **LineChartCard** (`components/cards/statistics/LineChart`): Displays trend data as a line chart with support for multiple series.
- **BarChartCard** (`components/cards/statistics/BarChart`): Displays categorical or time-bucket comparisons as a bar chart.
- **PieChartCard** (`components/cards/statistics/PieChart`): Displays distribution/breakdown data (e.g. sales by order status, customer tiers). Pass `innerRadius` (e.g. `innerRadius={40}`) for donut charts.
- **TableCard** (`components/cards/statistics/TableCard`): Displays ranked / top-N tabular data (e.g. top-selling products, low-stock items) with clickable resource link templates (e.g. `link: "/products/{id}"`).

### Chart Data Formats

#### Format 1: Cartesian Charts (`LineChartCard`, `BarChartCard`)
Expects the backend to return JSON data in the following format:

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
Expects an array of row objects (or `{ "data": [...] }`):

```json
[
  { "id": 1, "name": "Widget A", "sku": "WID-A", "totalSold": 150, "totalRevenue": 2999.50 },
  { "id": 2, "name": "Gadget B", "sku": "GAD-B", "totalSold": 95, "totalRevenue": 1425.00 }
]
```


- `xAxis`: Array of labels for the x-axis
- `series`: Array of data series, where each series has:
  - `data`: Array of numerical values
  - `label`: Display label for the series

For multiple lines on the same chart, add multiple series objects:

```json
{
  "xAxis": ["Jan", "Feb", "Mar", "Apr", "May", "Jun"],
  "series": [
    {
      "data": [10, 25, 30, 45, 40, 55],
      "label": "Sales"
    },
    {
      "data": [15, 20, 35, 40, 45, 50],
      "label": "Expenses"
    }
  ]
}
```

### Step 4: Create Chart Backend Handler

Create a backend handler that returns data in the chart format. The handler should query the database and transform the results.

**Tool call example:**
```javascript
write_file({
  shell: "backend",
  path: "Service/Analytics/MonthlySalesAggregator.php",
  content: `<?php

namespace App\Service\Analytics;

use App\Entity\Order;
use Doctrine\ORM\EntityManagerInterface;

class MonthlySalesAggregator implements AnalyticsAggregatorInterface
{
    public function __construct(
        private readonly EntityManagerInterface $entityManager
    ) {}

    public function getName(): string
    {
        return 'sales/monthly';
    }

    public function getValue(): array
    {
        $results = $this->entityManager
            ->getRepository(Order::class)
            ->createQueryBuilder('o')
            ->select('o.month as month, SUM(o.amount) as total')
            ->groupBy('o.month')
            ->orderBy('o.month')
            ->getQuery()
            ->getResult();

        $xAxis = array_map(fn($r) => $r['month'], $results);
        $data = array_map(fn($r) => (int) $r['total'], $results);

        return [
            'xAxis' => $xAxis,
            'series' => [
                [
                    'data' => $data,
                    'label' => 'Monthly Sales'
                ]
            ]
        ];
    }
}
`
})
```

### Step 5: Add Chart Widget to Dashboard

Update `views/dashboard/default.jsx` using `write_file` to add the chart widget inside `<Dashboard>`.

**Tool call example:**
```javascript
write_file({
  shell: "frontend",
  path: "views/dashboard/default.jsx",
  content: `'use client';

// project imports
import Dashboard from 'components/dashboard/Dashboard';
import KPICard from 'components/cards/statistics/KPICard';
import LineChartCard from 'components/cards/statistics/LineChart';

// ==============================|| DASHBOARD - DEFAULT ||============================== //

export default function DashboardDefault() {
  return (
    <Dashboard title="Dashboard">
      <KPICard
        title="Customers"
        endpoint="/analytics/customers/count"
        size={{ xs: 12, sm: 6, lg: 3 }}
      />
      <LineChartCard
        title="Monthly Sales"
        endpoint="/analytics/sales/monthly"
        size={{ xs: 12, lg: 6 }}
      />
    </Dashboard>
  );
}
`
})
```

---

## Dedicated Report Pages & Custom Analytics Views

When a requirement calls for a dedicated report page (e.g. date-range filtered order summaries, detailed audit logs, or custom tabular views beyond the dashboard), create the page using `write_page` and use the standalone `DataTable` component.

### `DataTable` Component

The `DataTable` component (`components/crud/DataTable`) provides a table with integrated search, filter inputs (including date range pickers and selects), pagination, and auto-refresh.

#### Props:
- `endpoint`: API endpoint (e.g., `"/analytics/orders/report"`). **Do NOT prefix with `/api`** — the frontend `apiService` automatically prefixes `/api`.
- `columns`: Ant Design table column definitions array
- `filters`: Array of filter definitions:
  - `{ name: "dateRange", type: "dateRange", label: "Date Range" }`
  - `{ name: "status", type: "select", label: "Status", options: { draft: "Draft", confirmed: "Confirmed" } }`
  - `{ name: "search", type: "text", label: "Search" }`
- `params`: Object of constant query parameters to pass to the endpoint

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

#### Step 1: Create the Backend Report Aggregator

**Tool call example:**
```javascript
write_file({
  shell: "backend",
  path: "Service/Analytics/OrderReportAggregator.php",
  content: `<?php

namespace App\Service\Analytics;

use App\Entity\Customer;
use App\Entity\Order;
use Doctrine\ORM\EntityManagerInterface;
use Doctrine\ORM\Query\Expr\Join;
use Symfony\Component\HttpFoundation\Request;

class OrderReportAggregator implements AnalyticsAggregatorInterface
{
    public function __construct(
        private readonly EntityManagerInterface $entityManager
    ) {}

    public function getName(): string
    {
        return 'orders/report';
    }

    public function getValue(): array
    {
        return $this->getValueWithRequest(Request::createFromGlobals());
    }

    public function getValueWithRequest(Request $request): array
    {
        $filters = $request->query->all('filters');
        $search = $request->query->get('search');

        $qb = $this->entityManager->createQueryBuilder()
            ->select(
                'o.id AS id,
                 o.orderNumber AS orderNumber,
                 c.name AS customerName,
                 o.orderDate AS orderDate,
                 o.status AS status,
                 COALESCE((SELECT SUM(i.quantity * i.unitPrice) FROM App\\\\Entity\\\\OrderItem i WHERE i.orderId = o.id), 0) AS totalAmount'
            )
            ->from(Order::class, 'o')
            ->innerJoin(Customer::class, 'c', Join::WITH, 'o.customerId = c.id');

        // Handle date range filter: filters[dateRange][0] and filters[dateRange][1]
        if (!empty($filters['dateRange'][0])) {
            $qb->andWhere('o.orderDate >= :dateFrom')
               ->setParameter('dateFrom', $filters['dateRange'][0]);
        }
        if (!empty($filters['dateRange'][1])) {
            $qb->andWhere('o.orderDate <= :dateTo')
               ->setParameter('dateTo', $filters['dateRange'][1]);
        }

        // Handle select or text filters: filters[status]
        if (!empty($filters['status'])) {
            $qb->andWhere('o.status = :status')
               ->setParameter('status', $filters['status']);
        }

        // Handle search query
        if (!empty($search)) {
            $qb->andWhere('o.orderNumber LIKE :search OR c.name LIKE :search')
               ->setParameter('search', '%' . $search . '%');
        }

        $qb->orderBy('o.orderDate', 'DESC');

        $results = $qb->getQuery()->getResult();

        return array_map(function ($row) {
            $orderDate = $row['orderDate'];
            if ($orderDate instanceof \\DateTimeInterface) {
                $orderDate = $orderDate->format('Y-m-d');
            }
            return [
                'id' => (int) $row['id'],
                'orderNumber' => $row['orderNumber'],
                'customerName' => $row['customerName'],
                'orderDate' => $orderDate,
                'status' => $row['status'],
                'totalAmount' => (float) $row['totalAmount'],
            ];
        }, $results);
    }
}
`
})
```

#### Step 2: Create the Dedicated Frontend Report Page

Use `write_page` with the `DataTable` component. The `endpoint` property points directly to `"/analytics/orders/report"` (matching `"/analytics/" + aggregator.getName()`):

```javascript
write_page({
  route: "/reports/orders",
  code: `'use client';

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
    { title: 'Order #', dataIndex: 'orderNumber', key: 'orderNumber' },
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
      render: (val) => '$' + Number(val || 0).toFixed(2),
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
      />
    </div>
  );
}
`
})
```

#### Step 3: Add to Navigation Menu

Add the report to the navigation menu using `write_menu`:
```javascript
write_menu({
  name: "Reports.OrderReport",
  route: "/reports/orders",
  icon: "BarChartOutlined",
  after: "Orders"
})
```

---

## Best Practices

1. **Always read before writing dashboard files**: Use `read_file` first on `views/dashboard/default.jsx` so you retain existing widgets inside `<Dashboard>`.
2. **Endpoint matching & 2-segment rule**: The `endpoint` prop in widgets and pages (e.g., `"/analytics/customers/count"`, `"/analytics/products/lowStock"`, `"/analytics/orders/report"`) must match `"/analytics/" + aggregator.getName()`. Aggregator names and endpoints must **strictly follow the 2-segment pattern** (`/analytics/<resource>/<metric>`). Never introduce extra slashes (such as `/analytics/products/lowStock/count`), as this will fail with a 404.
3. **Never create custom Symfony controllers for reports**: Never create an `AbstractController` or `#[Route('/api/analytics/...')]` class. The framework automatically dispatches `/api/analytics/{category}/{metric}` to any service implementing `AnalyticsAggregatorInterface` under `App\Service\Analytics\`. Use `getValueWithRequest(Request $request)` to read filter or search params.
4. **Never include `/api` in frontend endpoints**: Frontend requests go through `apiService` which already prefixes `/api`. Using `"/api/analytics/..."` will result in `"/api/api/analytics/..."` and fail with a 404. Always use `"/analytics/..."`.
5. **Chart data format**: All chart widgets use the documented JSON data formats (Cartesian, Distribution, or Tabular).
6. **Grid Sizing**: Widgets can declare their own 12-column layout sizing directly via the `size` prop (e.g., `size={{ xs: 12, sm: 6, lg: 3 }}` or `size={3}`).
7. **Optimized DB queries & QueryBuilder filtering**:
   - Perform calculations (e.g. `COUNT`, `SUM`, `AVG`) at the database layer via QueryBuilder rather than loading full entity collections into memory.
   - **Always use `andWhere()` (or `orWhere()`) instead of chaining multiple `where()` calls**: In Doctrine QueryBuilder, calling `->where(...)` multiple times overwrites prior conditions. Use `->where(...)` for the initial condition and subsequent `->andWhere(...)` for additional filters.
8. **Entity Associations and Joins**:
   - **Only `type: "table"` fields generate an automated, one-sided ORM association**: When a resource declares a child table with `targetEntity` (e.g., `Order` having `items` with `targetEntity: "OrderItem"`), `sync_schema` automatically generates `#[ORM\OneToMany]` on the parent entity. You can join directly from the parent:
     ```php
     // Valid: Order has an automated OneToMany association to items
     $qb = $this->entityManager->createQueryBuilder()
         ->select("DATE_FORMAT(o.orderDate, '%Y-%m') AS month, SUM(i.quantity * i.unitPrice) AS total")
         ->from(Order::class, 'o')
         ->innerJoin('o.items', 'i')
         ->where('o.orderDate >= :startDate')
         ->andWhere('o.status != :cancelled');
     ```
   - **Everything else has NO automated association and requires an explicit Join with `Join::WITH`**: Child entities (such as `OrderItem` back to `Order`), or foreign key relationships (such as `Order` to `Customer` via `customerId`), do not have automated ORM navigation properties. Attempting `->innerJoin('i.order', 'o')` or `->innerJoin('o.customer', 'c')` will fail with an association error. Instead, perform an explicit join specifying the entity class and the `Join::WITH` condition:
     ```php
     use Doctrine\ORM\Query\Expr\Join;
     use App\Entity\Order;
     use App\Entity\OrderItem;

     // Explicit join: querying from child OrderItem to parent Order via orderId
     $qb = $this->entityManager->createQueryBuilder()
         ->select("DATE_FORMAT(o.orderDate, '%Y-%m') AS month, SUM(i.quantity * i.unitPrice) AS total")
         ->from(OrderItem::class, 'i')
         ->innerJoin(Order::class, 'o', Join::WITH, 'i.orderId = o.id')
         ->where('o.orderDate >= :startDate')
         ->andWhere('o.status != :cancelled');
     ```