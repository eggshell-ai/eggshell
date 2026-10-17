# Custom Row Actions

This is the detailed reference for resource **row actions**. It is bundled as an
on-demand document of the `crud_creation` skill and should be loaded whenever a
requirement involves per-row operations, custom JavaScript behavior, or
popup/form actions.

```javascript
load_skill({ skillName: "crud_creation", document: "row_actions" })
```

Row actions are configured declaratively on a resource's `actions` array. There
are three kinds:

1. **Path actions** — POST to a backend route derived from an expression.
2. **Handler actions** — run custom JavaScript with an injected helper API.
3. **Form/popup actions** — open a reusable dialog, collect input, and submit to
   a custom endpoint.

Most row-level domain operations (activate/deactivate, mark contacted, approve,
cancel, reopen) are path actions. Use handler or form actions only when the
behavior genuinely cannot be expressed as a single backend POST.

---

## 1. Common action properties

Every action supports:

| Property | Required | Meaning |
|---|---|---|
| `permission` | no | Permission required to see/execute the action. Superusers always pass. |
| `labelExpression` | yes | JavaScript expression evaluated per row (with the record available as `data`) that returns the button label. A falsy result hides the button for that row. |
| `confirm` | no | When `true`, shows a confirmation dialog before running. |

Frontend visibility is convenience only. Backend checks are security and
correctness. See the invariants in sections 5 and 6.

---

## 2. Path actions

A path action appends an evaluated path to `<endpoint>/<record.id>` and issues a
POST.

```javascript
actions: [
  {
    permission: "leads.edit",
    actionExpression: "data.status == 'new' ? '/contacted' : null",
    labelExpression: "data.status == 'new' ? 'Mark Contacted' : null",
    confirm: true
  }
]
```

Behavior:

- `actionExpression` is evaluated per row with the record available as `data`;
- its result is appended to `<endpoint>/<record.id>`;
- a `null`/falsy action result hides the action for that row;
- the action uses POST;
- the grid refreshes after a successful action.

For `endpoint: "/leads"`, record id `42`, and
`actionExpression: "'/contacted'"`, the request is:

```text
POST /api/leads/42/contacted
```

through the normal backend resource route (`ResourceController`).

---

## 3. Handler actions (custom JavaScript)

A handler action runs a custom JavaScript expression when the button is clicked.
It is intended for client-side behavior that does not fit a single backend POST,
such as composing multiple API calls, navigating, or opening a custom popup.

```javascript
actions: [
  {
    permission: "orders.edit",
    labelExpression: "'Duplicate Order'",
    handler: "const copy = { ...data, id: undefined, orderNumber: data.orderNumber + '-COPY' }; await api.post('/orders', copy); message.success('Order duplicated'); await refresh();"
  }
]
```

### Injected helper API

The handler body is evaluated with a curated context. No DOM or arbitrary global
access is provided by convention — use only the helpers below.

| Helper | Description |
|---|---|
| `data` | The row record (same object exposed to expressions). |
| `record` | Alias of `data` for readability. |
| `api` | The API client: `api.get(url, config?)`, `api.post(url, body, config?)`, `api.put`, `api.patch`, `api.delete`. |
| `message` | Ant Design `message` API: `message.success(text)`, `message.error(text)`, `message.info(text)`. |
| `modal` | Ant Design `Modal` API: `modal.confirm({ title, content, onOk })`. |
| `refresh()` | Reloads the grid data. |
| `openForm(config)` | Opens a form/popup dialog (see section 4). `config` matches the `form` action property. |
| `close()` | Closes any dialog the action opened. |
| `values` | For form actions, the submitted form values (undefined for handler actions). |

Handlers may be `async` and use `await`. Exceptions are surfaced as an error
message and logged.

### Handler example: multi-step client action

```javascript
actions: [
  {
    permission: "invoices.edit",
    labelExpression: "data.status === 'draft' ? 'Issue Invoice' : null",
    confirm: true,
    handler: `
      const result = await api.post('/invoices/' + data.id + '/issue', {});
      message.success('Invoice ' + result.number + ' issued');
      await refresh();
    `
  }
]
```

Even for handler actions, the backend endpoint you call must enforce permission
and state — see sections 5 and 6.

---

## 4. Form / popup actions

A form action opens a reusable modal dialog built from a declarative field list,
collects user input, and submits it to a custom endpoint. This lets you add
"collect more data before acting" flows without writing bespoke React or CSS.

```javascript
actions: [
  {
    permission: "orders.edit",
    labelExpression: "data.status === 'draft' ? 'Adjust Items' : null",
    form: {
      title: "Adjust Order Items",
      endpoint: "/orders/{id}/adjust",
      method: "POST",
      submitLabel: "Apply",
      successMessage: "Order adjusted",
      values: { reason: "customer request" },
      fields: [
        { name: "quantity", type: "number", label: "Quantity", integer: true, min: 0, required: true },
        { name: "reason", type: "text", label: "Reason", required: true },
        { name: "notes", type: "textarea", label: "Notes" }
      ]
    }
  }
]
```

### Form configuration

| Property | Required | Meaning |
|---|---|---|
| `title` | no | Dialog title. |
| `endpoint` | no | Submission endpoint. Supports `{id}`, replaced with the record id. Defaults to a POST against `<endpoint>/<record.id>`. |
| `method` | no | HTTP method: `POST` (default), `PUT`, or `PATCH`. |
| `submitLabel` | no | Submit button label (default `Submit`). |
| `successMessage` | no | Message shown on success (default `Saved successfully`). |
| `values` | no | Static default values keyed by field name. |
| `fields` | yes | Field definitions, using the same shape as resource fields (`field.text(...)`, `field.number(...)`, etc.). |

The dialog reuses the standard form engine, so field types, validation, and
conditional visibility all behave exactly as they do in normal create/edit
forms. On success the dialog closes, the success message is shown, and the grid
refreshes.

### Field definitions

Form action fields use the **same field definition shape as resource fields** —
the JSON object form accepted by `sync_schema` (`{ name, type, label, ... }`).
Supported properties include `type`, `label`, `required`, `min`, `max`,
`options`, `visibleWhen`, `default`, and so on. See the CRUD skill reference for
the full field type list.

At runtime the generated resource file may express these fields using the
`field.*` builder (e.g. `field.number("quantity").label("Quantity").min(0).required()`);
both forms resolve to the same field configuration.

> **Note:** `sync_schema` persists actions into the generated resource file, so
> declare actions through `sync_schema` rather than editing generated files by
> hand — manual edits are overwritten on the next sync.

---

## 5. Frontend visibility is not backend enforcement

This is a critical invariant.

Suppose an action is only visible when:

```javascript
data.status == 'new'
```

A client can still manually send:

```text
POST /api/leads/42/contacted
```

even if the record is already `qualified`, `lost`, or `contacted`.

Therefore the backend route must verify the same state rule. Do not treat
`actionExpression` (or `labelExpression`) as authorization or validation. It is
UI behavior only.

---

## 6. Permission invariant

Unless a task deliberately specifies a different backend permission model, the
permission declared on the resource action must exactly match the permission
checked by the backend route.

If the resource says:

```javascript
permission: "leads.edit"
```

the backend must check:

```php
$this->denyAccessUnlessGranted('leads.edit');
```

Do not accidentally check an unrelated permission such as `users.edit`. Before
finishing, compare the permission strings character-for-character.

---

## 7. Backend custom route

Create the resource controller using `write_file` (never `write_controller`) and
add the custom route to it.

```javascript
write_file({
  shell: "backend",
  path: "Controller/LeadController.php",
  content: `<?php

namespace App\Controller;

use App\Entity\Lead;
use App\Resource\ResourceController;
use Symfony\Component\HttpFoundation\JsonResponse;
use Symfony\Component\HttpKernel\Exception\BadRequestHttpException;
use Symfony\Component\HttpKernel\Exception\NotFoundHttpException;
use Symfony\Component\Routing\Attribute\Route;

#[Route('/api/leads', name: 'leads.')]
final class LeadController extends ResourceController
{
    protected function getEntityClass(): string
    {
        return Lead::class;
    }

    protected function getResourceName(): string
    {
        return 'leads';
    }

    #[Route('/{id}/contacted', name: 'contacted', requirements: ['id' => '\\d+'], methods: ['POST'])]
    public function contacted(int $id): JsonResponse
    {
        $this->denyAccessUnlessGranted('leads.edit');

        $record = $this->entityManager
            ->getRepository($this->getEntityClass())
            ->find($id);

        if (!$record) {
            throw new NotFoundHttpException('Record not found');
        }

        if ($record->status !== 'new') {
            throw new BadRequestHttpException('Only new leads can be marked as contacted.');
        }

        $record->status = 'contacted';
        $this->entityManager->flush();

        return $this->json($record);
    }
}
`
})
```

For a **form action**, the submitted values arrive as the request JSON body.
Read them from the request and validate before mutating state:

```php
#[Route('/{id}/adjust', name: 'adjust', requirements: ['id' => '\\d+'], methods: ['POST'])]
public function adjust(int $id, Request $request): JsonResponse
{
    $this->denyAccessUnlessGranted('orders.edit');

    $data = json_decode($request->getContent(), true) ?? [];
    $quantity = (int) ($data['quantity'] ?? 0);

    // ... validate state, apply changes, flush ...

    return $this->json($record);
}
```

Remember to import any class you reference (e.g. `Symfony\Component\HttpFoundation\Request`).

This route checks the essential things:

1. permission;
2. record existence;
3. valid current state;
4. input validity (for form actions).

Only then does it mutate state.

---

## 8. State validation rule

Every state-dependent row action should ask:

> What must be true about the current record for this transition to be valid?

Examples:

- Mark Contacted → current status must be `new`;
- Approve → current status must be `draft`;
- Cancel → current status must be one of the cancellable states;
- Reactivate → current status must be `inactive`.

Mirror this state in the frontend expression for good UX, but enforce it again on
the backend.

---

## 9. Worked example: lead tracker actions

```javascript
actions: [
  {
    permission: "leads.edit",
    actionExpression: "data.status == 'new' ? '/contacted' : null",
    labelExpression: "data.status == 'new' ? 'Mark Contacted' : null",
    confirm: true
  },
  {
    permission: "leads.edit",
    labelExpression: "'Log Call'",
    form: {
      title: "Log a Call",
      endpoint: "/leads/{id}/calls",
      successMessage: "Call logged",
      fields: [
        {
          name: "outcome",
          type: "select",
          label: "Outcome",
          options: { answered: "Answered", voicemail: "Voicemail", no_answer: "No Answer" },
          required: true
        },
        { name: "notes", type: "textarea", label: "Notes" }
      ]
    }
  }
]
```

- The first action is a backend path action with a matching state check.
- The second opens a reusable form dialog and POSTs the outcome to
  `/api/leads/{id}/calls`.
- Both declare the same `leads.edit` permission that the backend enforces.

---

## Operating principles

1. Prefer path actions for single backend transitions.
2. Use handler actions only for genuinely client-side, multi-step behavior.
3. Use form actions when user input is required before the backend call.
4. Never treat frontend visibility as authorization — re-check on the backend.
5. Keep action permissions identical to the backend route's permission check.
6. Re-validate record state on the backend for every state-dependent action.
