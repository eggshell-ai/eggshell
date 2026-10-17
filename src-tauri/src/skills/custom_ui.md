# Custom UI and Custom JavaScript

This is the on-demand reference for adding behavior that goes beyond standard
CRUD: custom JavaScript, bespoke interactions, and reusable popups/forms. It is
bundled as a document of the `crud_creation` skill.

```javascript
load_skill({ skillName: "crud_creation", document: "custom_ui" })
```

The goal is to make rich custom behavior possible **without writing bespoke
React components or CSS**. Prefer declarative extensions of the resource system
first; drop to custom JavaScript only when a requirement genuinely cannot be
expressed declaratively.

---

## 1. Escalation ladder

Choose the lightest mechanism that satisfies the requirement:

1. **Declarative field/resource config** — the default. Covers most CRUD,
   validation, filters, computed fields, and display rules.
2. **Row actions** — per-row operations, path/handler/form actions. See the
   `row_actions` document.
3. **Custom JavaScript handlers** — client-side composition of API calls or
   navigation triggered from an action.
4. **Custom pages** — only when no resource/action mechanism can express the
   requirement (e.g. a highly specialized dashboard layout).

Do not skip straight to a custom page. The resource system already provides
tables, forms, filters, validation, delete confirmation, and record counts.

---

## 2. Custom JavaScript in row actions

Row actions can run custom JavaScript through the `handler` property:

```javascript
actions: [
  {
    permission: "products.edit",
    labelExpression: "'Sync Stock'",
    handler: "const res = await api.post('/products/' + data.id + '/sync-stock', {}); message.success('Stock synced: ' + res.stockQuantity); await refresh();"
  }
]
```

Handlers run with a curated helper API — see the `row_actions` document for the
full list (`data`, `api`, `message`, `modal`, `refresh`, `openForm`, `close`,
`values`). Handlers may be `async` and use `await`.

### Guidelines

- Keep handlers small and deterministic; move business logic to the backend.
- Always enforce permission and state on the backend the handler calls.
- Use `api` (the authenticated axios client) rather than raw `fetch` so the auth
  token and base URL are handled automatically.
- Report outcomes with `message.success` / `message.error`.
- Call `await refresh()` after any mutation so the grid reflects the new state.

---

## 3. Reusable popups and forms

Instead of hand-building a dialog, declare a **form action**:

```javascript
actions: [
  {
    permission: "customers.edit",
    labelExpression: "'Send Statement'",
    form: {
      title: "Send Account Statement",
      endpoint: "/customers/{id}/statements",
      submitLabel: "Send",
      successMessage: "Statement queued",
      values: { format: "pdf" },
      fields: [
        { name: "format", type: "select", label: "Format", options: { pdf: "PDF", csv: "CSV" }, required: true },
        { name: "from", type: "date", label: "From" },
        { name: "to", type: "date", label: "To" },
        { name: "email", type: "email", label: "Send To", required: true }
      ]
    }
  }
]
```

The dialog is generated from `fields` using the shared form engine, so it
supports:

- every standard field type (`text`, `textarea`, `email`, `phone`, `number`,
  `decimal`, `boolean`, `date`, `time`, `select`, `tags`, `file`);
- per-field validation (`required`, `min`, `max`, `options`, etc.);
- conditional visibility (`visibleWhen`);
- default values via `values` and per-field `default`.

On submit, the collected values are sent to `endpoint` (with `{id}` replaced by
the record id) using the configured `method`. On success the dialog closes, the
success message appears, and the grid refreshes.

This avoids writing custom React components, modals, form state, or CSS for
common "collect input then act" flows.

---

## 4. Choosing between handler and form actions

| Need | Use |
|---|---|
| Collect structured input, then POST | Form action |
| Fire a single backend transition with no input | Path action |
| Compose multiple API calls / navigate | Handler action |
| Open a dialog from a handler | `openForm(...)` helper |

For example, a handler can open the same reusable form programmatically:

```javascript
actions: [
  {
    permission: "orders.edit",
    labelExpression: "'Bulk Update'",
    handler: "openForm({ title: 'Bulk Update', endpoint: '/orders/bulk', fields: [{ name: 'ids', type: 'textarea', label: 'Order IDs', required: true }] });"
  }
]
```

---

## 5. Backend contract

Every custom action that mutates data must call a backend endpoint that:

1. checks permission (`denyAccessUnlessGranted('<permission>')`);
2. verifies the record exists and is in a valid state;
3. validates input (for form actions, read the JSON body);
4. performs the mutation and flushes.

Add the route to the resource's `ResourceController` subclass with `write_file`.
See the `row_actions` document for complete controller examples.

Do not rely on frontend expressions or handler logic for security.

---

## 6. When a custom page is truly required

If a requirement cannot be expressed with resources, fields, and actions — for
example a highly bespoke visualization with no CRUD semantics — create a custom
page under `app/(dashboard)/<route>/page.tsx` (beginning with `'use client';`)
and add a matching menu entry. Note that a custom page does **not** inherit the
automatic resource count or standard CRUD behaviors, so only take this path when
the standard mechanisms genuinely cannot satisfy the requirement.

---

## Operating principles

1. Prefer declarative config, then actions, then custom JS, then custom pages.
2. Use form actions for "collect input then submit" flows — avoid bespoke dialogs.
3. Use handler actions for client-side orchestration only.
4. Always enforce permission and state on the backend.
5. Refresh the grid after mutations.
6. Keep custom JavaScript small; push business logic to the backend.
