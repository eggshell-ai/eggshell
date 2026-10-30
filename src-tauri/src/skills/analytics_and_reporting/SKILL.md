---
name: analytics_and_reporting
description: Teaches the agent how to create backend analytics aggregators and dashboards
category: analytics
tags:
  - analytics
  - reporting
  - dashboard
---

# Analytics and Reporting Skill

This skill explains how to build analytics and reporting capabilities in the system. It covers creating backend analytics aggregator services and adding widgets to the frontend dashboard.

## Process

1. Create a **Backend Aggregator**
Create a new aggregator service in the backend under `Service/Analytics/` using `write_file`.

**Example:**
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

The method can return a JSON seriazable value (e.g. number, string or array of objects). It will be serialized automatically before returning.

2. Write the Frontend Consumer

We can then either build a report, or a dashboard widget. Here's an example of a dashboard widget:

**Example:**
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
        endpoint="/customers/count"
      />
    </Dashboard>
  );
}
`
})
```

We can also use `patch_file` if we want to make a small edit in an existing dashboard.

## Reference Documents

These documents define how to implement different requirements

Use `load_skill` tool to load the files.
Example
```javascript
load_skill({
  skillName: "analytics_and_reporting",
  path: "docs/dashboard.md"
})
```

- `docs/dashboard.md`: Defines how to create a dashboard with custom widgets

- `docs/reports.md`: Defines how to build a standalone report with optional filters.

## Backend Aggregators

Backend aggregators are similar for both types of reports, here's their detailed documentation:

### Structure
- `getName(): string`: Returns a unique identifier for the current API path, must be in the format of `module/id`, and can contain two parts only (e.g. sales/default, sales/revenue, sales/customer-wise)

- `getValue(): mixed`: Must return a JSON serializable value (number, boolean, object, array of objects, strings, etc.). Prefer an empty array or `0` over `null` to indicate no data.

### Entity Naming Conventions

When querying the database in backend aggregators:
- Entities live under the `App\Entity` namespace.
- Entity class names are the **singular**, **UpperCamelCase** version of the resource name.

Examples:
- Resource `customers` &rarr; `App\Entity\Customer`
- Resource `orders` &rarr; `App\Entity\Order`
- Resource `invoices` &rarr; `App\Entity\Invoice`
- Resource `product_categories` &rarr; `App\Entity\ProductCategory`

### Creation
Utilize `write_file({
  shell: "backend",
  path: "Service/Analytics/<NameAggregator>.php",
  content: "<?php ..."
})` to write a new aggregator. If you want to change an existing aggregator, prefer `patch_file` to make precise edits

### Common Patterns

**Monthly Trend**
The project has `DATE_FORMAT` from DQL extensions pre-installed, and computed columns are mirrored in the database, so the same works if `o.amount` is a virtual / computed column.
```php
<?php

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
          ->select("DATE_FORMAT(o.date, '%Y-%m') AS month, SUM(o.amount) AS total") // Use `YEAR(o.date)` for year-wise data
          ->groupBy('month')
          ->orderBy('month', 'ASC')
          ->getQuery()
          ->getResult();

      $xAxis = array_map(fn($r) => $r['month'], $results);
      $data = array_map(fn($r) => (float) $r['total'], $results);

      return [
          'xAxis' => $xAxis,
          'series' => [
              [
                  'data' => $data,
                  'label' => 'Monthly Sales',
              ],
          ],
      ];
  }
}
```

**Filtered Data**: Dispaly tabular data for a specific date range / entity

```php
<?php

namespace App\\Service\\Analytics;

use App\\Entity\\Customer;
use App\\Entity\\Order;
use Doctrine\\ORM\\EntityManagerInterface;
use Doctrine\\ORM\\Query\\Expr\\Join;
use Symfony\\Component\\HttpFoundation\\Request;

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
                 o.totalAmount AS totalAmount'
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

        $qb->groupBy('o.id, c.name')
           ->orderBy('o.orderDate', 'DESC');

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
```

Now, we are considering that `orders` has a field called `customerId`, defined like this:
```json
{
  "name" "customerId",
  "type": "select",
  "resource": "customers"
}
```

We convert it to entity name, and apply an explicit join since relationships aren't added automatically.