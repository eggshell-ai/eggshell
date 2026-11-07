---
name: planning
description: Defines how the planning agent must describe each buildable artifact — entities, dashboards, reports and workflows — and gives an example plan
category: planning
tags:
  - planning
  - breakdown
  - entities
  - dashboards
  - reports
  - workflows
---

# Planning Skill

When breaking a requirement into sections, every section must describe its artifact in the exact
terms below. This is what the implementation agent will read as its brief, so be specific: never
say "a dashboard" or "appropriate reports" — name the widgets, filters and columns.

## Entities

For every entity, define the entity with its name and fields:

- One line per entity: business name and its `<resource_name>` (plural, snake_case), e.g.
  `Sales — sales`.
- Fields, each on a separate line: field name (camelCase), inferred type from
  `text, textarea, email, phone, number, decimal, boolean, date, file, tags, table`,
  and `required` / `unique` markers only when the business meaning demands it.
- A field that references another entity is a `select` whose name ends in `Id`
  (e.g. `customerId`). Do not repeat the same foreign key on both sides.
- Keep every field list short and complete. Do not invent fields nobody asked for.
- If a value must be calculated (e.g. a line-item subtotal), call it out and say it should be a
  computed column rather than a plain field.
- When a record naturally has line items (orders with products, sales with items, vouchers with
  ledger entries), define the child entity in the SAME section as the parent. The child is a
  full entity with its own fields, including a reference back to the parent (e.g. `saleId`),
  and the parent gets a `table`-type field (e.g. `items`) that maps to the child via that
  reference. Aggregate amounts (e.g. a sale's `totalAmount`) should be computed from the child
  rows, not typed by hand.

Example — a sale with line items:

```
Sales — sales
- customerId (select, required) → customers
- saleDate (date, required)
- totalAmount (decimal, required) — computed as the sum of the items' subtotals
- status (select: pending / paid / partially_paid, required)
- items (table) → sale_items, mapped on saleId

Sale Items — sale_items
- saleId (select, required) → sales
- productId (select, required) → products
- quantity (number, required, min 1)
- unitPrice (decimal, required) — auto-lookup from the product's price
- subtotal (decimal, required) — computed: quantity × unitPrice
```

## Dashboards

Do not say "add a dashboard". Define the actual widgets that should display, each as one of:

- A **KPI card**: widget title, a short sentence on what it counts or sums, and the aggregator it
  needs, e.g. `KPI card "Customers today" — count of customers created today (aggregator
  customers/count-today)`.
- A **chart**: the chart's purpose, what the xAxis is, what the series measures, e.g.
  `Line chart of monthly sales — xAxis = month, series = sum of totalAmount (aggregator
  sales/monthly)`.
- A **table**: the columns it should show and any default sort.

Mention per widget which entity it aggregates from, so the implementer can write the right
backend aggregator.

## Reports

Reports are tabular views with optional filters. For each report define:

- The **columns**: field name and label, in display order.
- The **filters**: each filter's field and kind — a date range, a select over fixed options
  (name them), or a free-text search over named fields.

Example:

```
Payments Report — filter by date range and payment method (cash/bank/card_transfer); columns:
payment date, sale number, method, amount, status.
```

## Workflows

Define workflows in terms of **actions** on existing screens — where a control lives, what the
user does with it, and the rules applied. Never describe a workflow as "support payments";
spell out the interaction.

Example — the user asks for a payment system against sales:

```
### Receive Payment (workflow on Sales)
- Add a "Receive Payment" row button on each sale in the Sales list.
- Clicking it opens a popup where the user:
  - selects the payment method between Cash, Bank or Card Transfer;
  - enters the amount received;
  - confirms with an "Add Payment" button.
- Validation: the entered amount must be greater than 0 and must not exceed the sale's
  remaining amount (totalAmount minus payments already received).
- Each payment updates the sale's status automatically: fully covered → paid,
  partly covered → partially_paid.
```

When a section is a CRUD plus a workflow, put the workflow actions under the same section so the
implementing agent builds them together.

## Example Plan

This is the expected shape — adapt, never copy blindly. Each section defines the end
result, not how to build it; the implementation agent works out the steps:

```
## Overview
A sales module: record sales with line items to customers, receive full or partial
payments, and inspect sales activity.

## 1. Customers CRUD
Give the user a place to record and manage customers.

### Entity & Fields
Customers — customers
- name (text, required)
- phone (phone)
- email (email)

### Screen & Menu
- Route `/customers`, sidebar `Sales > Customers`, permission `customers.*`

## 2. Sales CRUD
Record sales against customers with multiple product line items.

### Entity & Fields
Sales — sales
- customerId (select, required) → customers
- saleDate (date, required)
- totalAmount (decimal, required) — computed as the sum of the items' subtotals
- status (select: pending / partially_paid / paid, required)
- items (table) → sale_items, mapped on saleId

Sale Items — sale_items
- saleId (select, required) → sales
- productId (select, required) → products
- quantity (number, required, min 1)
- unitPrice (decimal, required) — auto-lookup from the product's price
- subtotal (decimal, required) — computed: quantity × unitPrice

### Screen & Menu
- Route `/sales`, sidebar `Sales > Sales`, permission `sales.*`

## 3. Receive Payment workflow
Let the user receive full or partial payments against a sale.

### Payments entity
Payments — payments
- saleId (select, required) → sales
- method (select: cash / bank / card_transfer, required)
- amount (decimal, required)
- date (date, required)

### Actions
- "Receive Payment" row button on each sale in the Sales list.
- Popup: method select (Cash / Bank / Card Transfer), amount input, Add Payment button.
- Rules: amount > 0, amount ≤ remaining amount (totalAmount minus payments received);
  sale status updates to partially_paid or paid automatically.
```
