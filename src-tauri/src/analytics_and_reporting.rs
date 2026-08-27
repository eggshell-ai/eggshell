use super::Skill;

// Paste the full analytics and reporting skill instructions in this constant.
const CONTENT: &str = "# Analytics and Reporting Skill

This skill explains how to build analytics and reporting capabilities in the system. It covers creating backend analytics aggregator services and adding widgets to the frontend dashboard.

## Overview

Creating analytics metrics and displaying them on the dashboard involves two primary parts:

1. **Backend Aggregator**: A PHP service class in `Service/Analytics` implementing `AnalyticsAggregatorInterface` that queries the database using Doctrine ORM.
2. **Frontend Dashboard Widget**: Updating `views/dashboard/default.jsx` by adding `KPICard` widgets inside the `<Dashboard>` container.

To read and write these files across shells, use the `read_file` and `write_file` tools.

---

## Tooling & File Access

Use the `read_file` and `write_file` tools to interact with frontend and backend files:

- **`read_file`**:
  - `shell`: `\"frontend\"` or `\"backend\"`
  - `path`: Relative path inside `src` directory (e.g., `views/dashboard/default.jsx` or `Service/Analytics/CustomerCountAggregator.php`)
- **`write_file`**:
  - `shell`: `\"frontend\"` or `\"backend\"`
  - `path`: Relative path inside `src` directory
  - `content`: The complete updated content to write

---

## Entity Naming Conventions

When querying the database in backend aggregators:
- Entities live under the `App\\Entity` namespace.
- Entity class names are the **singular**, **UpperCamelCase** version of the resource name.

### Examples:
- Resource `customers` &rarr; `App\\Entity\\Customer`
- Resource `orders` &rarr; `App\\Entity\\Order`
- Resource `invoices` &rarr; `App\\Entity\\Invoice`
- Resource `product_categories` &rarr; `App\\Entity\\ProductCategory`

---

## Step-by-Step Implementation

### Step 1: Create Backend Analytics Aggregator

Create a new aggregator service in the backend under `Service/Analytics/` using `write_file`.

**Tool call example:**
```javascript
write_file({
  shell: \"backend\",
  path: \"Service/Analytics/CustomerCountAggregator.php\",
  content: `<?php

namespace App\\Service\\Analytics;

use App\\Entity\\Customer;
use Doctrine\\ORM\\EntityManagerInterface;

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
- Must implement `App\\Service\\Analytics\\AnalyticsAggregatorInterface`.
- `getName()`: Returns the unique metric key path (e.g., `'customers/count'`), corresponding to the endpoint path `/analytics/{getName()}`.
- `getValue()`: Calculates and returns the calculated aggregate metric value (e.g., total count, sum, average, or enter array of objects).

---

### Step 2: Read Frontend Dashboard

Before modifying the dashboard, read `views/dashboard/default.jsx` using `read_file` to see existing widgets and preserve their structure.

**Tool call example:**
```javascript
read_file({
  shell: \"frontend\",
  path: \"views/dashboard/default.jsx\"
})
```

---

### Step 3: Add Widget to Dashboard

Update `views/dashboard/default.jsx` using `write_file` to add the new `KPICard` widget inside `<Dashboard>`.

The `<Dashboard>` container automatically handles title headers, refresh actions, and automated cache registration/invalidation. Widgets declare their grid size via the `size` prop (using a 12-column grid system, e.g. `size={{ xs: 12, sm: 6, lg: 3 }}` or `size={3}`).

**Tool call example:**
```javascript
write_file({
  shell: \"frontend\",
  path: \"views/dashboard/default.jsx\",
  content: `'use client';

// project imports
import Dashboard from 'components/dashboard/Dashboard';
import KPICard from 'components/cards/statistics/KPICard';

// ==============================|| DASHBOARD - DEFAULT ||============================== //

export default function DashboardDefault() {
  return (
    <Dashboard title=\"Dashboard\">
      <KPICard
        title=\"Customers\"
        endpoint=\"/analytics/customers/count\"
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

The following chart widgets are available for displaying analytics data:

- **LineChart**: Displays data as a line chart with support for multiple series

**Note**: All chart widgets use the same data format (Format 1 below).

### Chart Data Format

All chart widgets expect the backend to return JSON data in the following format:

```json
{
  \"xAxis\": [\"Jan\", \"Feb\", \"Mar\", \"Apr\", \"May\", \"Jun\"],
  \"series\": [
    {
      \"data\": [10, 25, 30, 45, 40, 55],
      \"label\": \"Sales\"
    }
  ]
}
```

- `xAxis`: Array of labels for the x-axis
- `series`: Array of data series, where each series has:
  - `data`: Array of numerical values
  - `label`: Display label for the series

For multiple lines on the same chart, add multiple series objects:

```json
{
  \"xAxis\": [\"Jan\", \"Feb\", \"Mar\", \"Apr\", \"May\", \"Jun\"],
  \"series\": [
    {
      \"data\": [10, 25, 30, 45, 40, 55],
      \"label\": \"Sales\"
    },
    {
      \"data\": [15, 20, 35, 40, 45, 50],
      \"label\": \"Expenses\"
    }
  ]
}
```

### Step 4: Create Chart Backend Handler

Create a backend handler that returns data in the chart format. The handler should query the database and transform the results.

**Tool call example:**
```javascript
write_file({
  shell: \"backend\",
  path: \"Service/Analytics/MonthlySalesAggregator.php\",
  content: `<?php

namespace App\\Service\\Analytics;

use App\\Entity\\Order;
use Doctrine\\ORM\\EntityManagerInterface;

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
  shell: \"frontend\",
  path: \"views/dashboard/default.jsx\",
  content: `'use client';

// project imports
import Dashboard from 'components/dashboard/Dashboard';
import KPICard from 'components/cards/statistics/KPICard';
import LineChartCard from 'components/cards/statistics/LineChart';

// ==============================|| DASHBOARD - DEFAULT ||============================== //

export default function DashboardDefault() {
  return (
    <Dashboard title=\"Dashboard\">
      <KPICard
        title=\"Customers\"
        endpoint=\"/analytics/customers/count\"
        size={{ xs: 12, sm: 6, lg: 3 }}
      />
      <LineChartCard
        title=\"Monthly Sales\"
        endpoint=\"/analytics/sales/monthly\"
        size={{ xs: 12, lg: 6 }}
      />
    </Dashboard>
  );
}
`
})
```

---

## Best Practices

1. **Always read before writing dashboard files**: Use `read_file` first on `views/dashboard/default.jsx` so you retain existing widgets inside `<Dashboard>`.
2. **Endpoint matching**: The `endpoint` prop in widgets (e.g., `\"/analytics/customers/count\"`) must match `\"/analytics/\" + aggregator.getName()`.
3. **Chart data format**: All chart widgets use the same format with `xAxis` and `series` arrays. Ensure your backend handlers return data in this structure.
4. **Grid Sizing**: Widgets can declare their own 12-column layout sizing directly via the `size` prop (e.g., `size={{ xs: 12, sm: 6, lg: 3 }}` or `size={3}`).
5. **Optimized DB queries**: Perform calculations (e.g. `COUNT`, `SUM`, `AVG`) at the database layer via QueryBuilder rather than loading full entity collections into memory.";

/// Skill for creating backend analytics aggregators and dashboards.
pub struct AnalyticsAndReportingSkill {
    tags: Vec<String>,
}

impl AnalyticsAndReportingSkill {
    pub fn new() -> Self {
        Self {
            tags: vec!["analytics", "reporting", "dashboard"]
                .into_iter()
                .map(str::to_string)
                .collect(),
        }
    }
}

impl Default for AnalyticsAndReportingSkill {
    fn default() -> Self { Self::new() }
}

impl Skill for AnalyticsAndReportingSkill {
    fn name(&self) -> &str { "analytics_and_reporting" }
    fn description(&self) -> &str {
        "Teaches the agent how to create backend analytics aggregators and dashboards"
    }
    fn content(&self) -> &str { CONTENT }
    fn category(&self) -> Option<&str> { Some("analytics") }
    fn tags(&self) -> &[String] { &self.tags }
}
