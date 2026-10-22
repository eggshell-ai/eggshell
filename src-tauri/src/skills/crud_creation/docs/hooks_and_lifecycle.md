# Resource Hooks and Save Lifecycle

This is the detailed reference for **customizing the save flow** of a resource:
running code before or after a record is saved, adding validation that the
declarative resource schema cannot express, and taking full control of the save
operation.

It is bundled as an on-demand document of the `crud_creation` skill. Load it
before implementing or modifying any hook, lifecycle behavior, or custom save
logic:

```javascript
load_skill({ skillName: "crud_creation", document: "hooks_and_lifecycle" })
```

---

## 1. Overview

Every `store` (create) and `update` request handled by `ResourceController`
follows the same ordered lifecycle. Knowing the order is essential for choosing
the correct extension point.

```text
store():
  fillEntity()
  beforeSave()                       <- controller override
  runBeforeSaveHooks()               <- BeforeSavesResource hooks
  validator->validate()
  runValidationHooks()               <- ValidatesResource hooks
  persist() + flush()
  saveChildRelations() + flush()
  runAfterSaveHooks()                <- AfterSavesResource hooks
  serialize response

update():
  fillEntity()
  beforeSave()                       <- controller override
  runBeforeSaveHooks()               <- BeforeSavesResource hooks
  validator->validate()
  runValidationHooks()               <- ValidatesResource hooks
  saveChildRelations() + flush()
  runAfterSaveHooks()                <- AfterSavesResource hooks
  serialize response
```

Choose the lightest extension point that satisfies the requirement:

| Requirement | Use |
|---|---|
| Single-field rule (required, unique, size, format) | Declarative field config |
| Cross-field / conditional validation returning errors | `ValidatesResource` hook (+ frontend mirror) |
| Prepare/normalize/derive values before persisting | `BeforeSavesResource` hook, or `beforeSave()` override |
| Work that needs the saved record's primary key (related records, audit rows) | `AfterSavesResource` hook |
| Save flow that cannot follow the standard pipeline at all | Override `store()` / `update()` directly |

There are two ways to add backend logic: **auto-discovered hook classes** (the
preferred, reusable mechanism that mirrors `ValidatesResource`) and **controller
method overrides** (a single-resource escape hatch).

---

## 2. Hook classes and auto-discovery

Custom hooks live under the backend path:

```text
src/Resource/Hooks/<resourceName>/
```

With `write_file`, the backend-relative path is:

```text
Resource/Hooks/<resourceName>/<ClassName>.php
```

Rules:

- one class per file;
- the class name must exactly match the file name (without `.php`);
- the namespace is `App\Resource\Hooks\<resourceName>`;
- a hook class may implement **any combination** of the hook interfaces
  (`ValidatesResource`, `BeforeSavesResource`, `AfterSavesResource`) to
  participate in the corresponding lifecycle points.

`ResourceController` discovers every valid class in that directory automatically
and runs the relevant interface method. There is no registry to edit on the
backend.

The `<resourceName>` segment must match `getResourceName()` exactly (e.g.
`leads`, `orders`), or the hook is ignored.

Hook classes are instantiated with **no constructor arguments**, so they cannot
use constructor dependency injection. When a hook needs the database, use the
`EntityManagerInterface` that `ResourceController` passes to the before-save and
after-save methods.

---

## 3. Validation hooks (backend)

For validation that cannot be represented declaratively, implement
`ValidatesResource`.

```php
<?php

namespace App\Resource\Hooks\orders;

use App\Resource\Hook\ValidatesResource;

final class OrderValidationHook implements ValidatesResource
{
    public function validate(object $entity, array $data, string $action): array
    {
        $errors = [];

        // custom rules

        return $errors;
    }
}
```

The hook runs during `store` and `update` **after** standard declarative
validation.

Return:

```php
[
    'fieldName' => 'Error message'
]
```

or arrays of messages if needed.

Return an empty array when validation passes.

Custom validation errors produce the normal validation response and are displayed
by `ResourcePage` beside the relevant field. Keep errors field-addressable.

Do not put resource validation rules in controller actions merely because custom
code is needed. Validation hooks are the designated mechanism.

---

### 3.1 Cross-field validation recipe: at least one of email or phone

Requirement:

> At least one of email or phone must be provided.

Do not write checks that accidentally accept both fields being absent.
Normalize empty strings and whitespace.

#### Backend

```javascript
write_file({
  shell: "backend",
  path: "Resource/Hooks/leads/LeadValidationHook.php",
  content: `<?php

namespace App\Resource\Hooks\leads;

use App\Resource\Hook\ValidatesResource;

final class LeadValidationHook implements ValidatesResource
{
    public function validate(object $entity, array $data, string $action): array
    {
        $errors = [];

        $email = trim((string) ($entity->email ?? ''));
        $phone = trim((string) ($entity->phone ?? ''));

        if ($email === '' && $phone === '') {
            $errors['email'] = 'Enter at least an email address or phone number.';
            $errors['phone'] = 'Enter at least an email address or phone number.';
        }

        return $errors;
    }
}
`
})
```

#### Frontend

```javascript
write_file({
  shell: "frontend",
  path: "resources/hooks/leads/leadValidationHook.ts",
  content: `import type { ResourceValidationHook } from '../../../types/resource';

const leadValidationHook: ResourceValidationHook = {
  validate: (data, action, record) => {
    const errors: Record<string, string | string[]> = {};

    const email = String(data?.email ?? '').trim();
    const phone = String(data?.phone ?? '').trim();

    if (!email && !phone) {
      const message = 'Enter at least an email address or phone number.';
      errors.email = message;
      errors.phone = message;
    }

    return errors;
  },
};

export default leadValidationHook;
`
})
```

The two checks are intentionally equivalent:

```text
backend: email == empty AND phone == empty
frontend: !email AND !phone
```

Do not use an expression such as:

```javascript
!(data?.email || data.phone == null)
```

because that changes the logical meaning and can accept invalid input.

---

### 3.2 Conditional-required recipe

Requirement:

> If source is `other`, show `sourceDescription` and require it.

This requires two parts:

1. conditional frontend visibility;
2. frontend + backend validation.

#### Resource field

```javascript
{
  name: "sourceDescription",
  type: "text",
  label: "Other Source Description",
  visibleWhen: "data.source == 'other'"
}
```

Do **not** set:

```javascript
required: true
```

if the field is required only under that condition, because unconditional
`required: true` would make it required for every source.

Instead, enforce the condition with hooks.

#### Backend rule

```php
$source = (string) ($entity->source ?? '');
$description = trim((string) ($entity->sourceDescription ?? ''));

if ($source === 'other' && $description === '') {
    $errors['sourceDescription'] = 'Describe the other lead source.';
}
```

#### Frontend rule

```typescript
const source = String(data?.source ?? '');
const description = String(data?.sourceDescription ?? '').trim();

if (source === 'other' && !description) {
  errors.sourceDescription = 'Describe the other lead source.';
}
```

These rules must remain equivalent.

Conditional visibility is not validation. A malicious or custom client can call
the backend without respecting `visibleWhen`, so the backend hook must enforce
the rule.

---

## 4. Before-save hooks

Use a before-save hook to run code **after** the entity has been populated and
validated, but **before** it is persisted. The entity does not yet have a
primary key for new records.

There are two ways to do this.

### 4.1 Auto-discovered `BeforeSavesResource` hook (preferred)

Implement `BeforeSavesResource` in a hook class under
`Resource/Hooks/<resourceName>/`.

```php
<?php

namespace App\Resource\Hooks\orders;

use App\Resource\Hook\BeforeSavesResource;
use Doctrine\ORM\EntityManagerInterface;

final class OrderBeforeSaveHook implements BeforeSavesResource
{
    public function beforeSave(
        object $entity,
        array $data,
        string $action,
        EntityManagerInterface $entityManager
    ): void {
        // Normalise or derive values before persisting.
    }
}
```

Example: snap a `balanceDue` value from the payload before persisting.

```php
<?php

namespace App\Resource\Hooks\invoices;

use App\Resource\Hook\BeforeSavesResource;
use Doctrine\ORM\EntityManagerInterface;

final class InvoiceBeforeSaveHook implements BeforeSavesResource
{
    public function beforeSave(
        object $entity,
        array $data,
        string $action,
        EntityManagerInterface $entityManager
    ): void {
        $total = 0.0;
        foreach (($data['items'] ?? []) as $item) {
            if (!is_array($item)) {
                continue;
            }
            $total += ((float) ($item['quantity'] ?? 0)) * ((float) ($item['unitPrice'] ?? 0));
        }

        $entity->balanceDue = number_format($total, 2, '.', '');
    }
}
```

A hook class may implement several interfaces at once:

```php
use Doctrine\ORM\EntityManagerInterface;

final class InvoiceHooks implements ValidatesResource, BeforeSavesResource, AfterSavesResource
{
    public function validate(object $entity, array $data, string $action): array { /* ... */ }

    public function beforeSave(
        object $entity,
        array $data,
        string $action,
        EntityManagerInterface $entityManager
    ): void { /* ... */ }

    public function afterSave(
        object $entity,
        array $data,
        string $action,
        EntityManagerInterface $entityManager
    ): void { /* ... */ }
}
```

### 4.2 Controller override `beforeSave()`

`ResourceController` also exposes a protected `beforeSave()` method that a
resource controller can override. This is appropriate when the logic is
specific to one resource and benefits from the controller's injected services
(for example, a password hasher).

```php
<?php

namespace App\Controller;

use App\Entity\User;
use App\Resource\ResourceController;
use Symfony\Component\PasswordHasher\Hasher\UserPasswordHasherInterface;
use Symfony\Component\Routing\Attribute\Route;

#[Route('/api/users', name: 'users.')]
final class UserController extends ResourceController
{
    public function __construct(
        /* ...parent dependencies... */
        private readonly UserPasswordHasherInterface $passwordHasher
    ) {
        parent::__construct(/* ... */);
    }

    protected function beforeSave(object $entity, array $data, string $action): void
    {
        /** @var User $entity */

        if (isset($data['password']) && !empty($data['password'])) {
            $entity->password = $this->passwordHasher->hashPassword($entity, $data['password']);
        }
    }
}
```

Use the auto-discovered hook (4.1) when the behavior is reusable and does not
need controller services. Use the override (4.2) when you need injected
dependencies or resource-local logic. Both run at the same lifecycle point.

Before-save hooks cannot return validation errors; use a `ValidatesResource`
hook for anything that must produce field errors.

---

## 5. After-save hooks

Use an after-save hook to run code **after** the record (and any child line
items) have been persisted and flushed. The entity now has its primary key,
which makes this the correct place for work that depends on the saved record
existing in the database: updating related records, cascading state, writing
audit or analytics rows, and other side effects.

Implement `AfterSavesResource` in a hook class under
`Resource/Hooks/<resourceName>/`.

```php
<?php

namespace App\Resource\Hooks\orders;

use App\Resource\Hook\AfterSavesResource;
use Doctrine\ORM\EntityManagerInterface;

final class OrderAfterSaveHook implements AfterSavesResource
{
    public function afterSave(
        object $entity,
        array $data,
        string $action,
        EntityManagerInterface $entityManager
    ): void {
        // The record already has its primary key here.
    }
}
```

Example: when an order is created or updated, stamp the customer's last order
date.

```php
<?php

namespace App\Resource\Hooks\orders;

use App\Entity\Customer;
use App\Resource\Hook\AfterSavesResource;
use Doctrine\ORM\EntityManagerInterface;

final class OrderAfterSaveHook implements AfterSavesResource
{
    public function afterSave(
        object $entity,
        array $data,
        string $action,
        EntityManagerInterface $entityManager
    ): void {
        /** @var \App\Entity\Order $entity */

        if ($entity->customerId === null) {
            return;
        }

        $customer = $entityManager
            ->getRepository(Customer::class)
            ->find($entity->customerId);

        if ($customer === null) {
            return;
        }

        $customer->lastOrderDate = date('Y-m-d');
        $entityManager->flush();
    }
}
```

Important details:

- the record has already been flushed, so **mutations made here are not saved
  automatically** — persist and flush explicitly when you change more entities;
- the hook runs for both `store` and `update`; use `$action` when behavior must
  differ;
- if the after-save work touches other aggregates, keep it idempotent where
  possible.

---

## 6. Writing the save method from scratch

When the standard pipeline genuinely cannot express the required save behavior,
override `store()` and/or `update()` directly in the resource controller.

Prefer the lifecycle hooks above when they suffice. Only take over the save
method when you must control the ordering, short-circuit validation, bypass
`fillEntity()`, or perform a non-standard transactional flow.

Reuse the inherited helpers so you keep the standard behavior:

- `$this->entityManager`;
- `$this->validator`;
- `$this->fillEntity($entity, $data)`;
- `$this->handleFileUploads($request, $data)`;
- `$this->runValidationHooks(...)` / `$this->hookValidationErrorResponse(...)`;
- `$this->validationErrorResponse(...)`;
- `$this->loadChildRelations(...)` / `$this->loadRelationTitles(...)`;
- `$this->checkPermission('store')`.

Example: a controller that fully replaces `store()` to apply a custom
transactional flow.

```php
<?php

namespace App\Controller;

use App\Entity\Invoice;
use App\Resource\ResourceController;
use Symfony\Component\HttpFoundation\JsonResponse;
use Symfony\Component\HttpFoundation\Request;
use Symfony\Component\HttpFoundation\Response;
use Symfony\Component\Routing\Attribute\Route;

#[Route('/api/invoices', name: 'invoices.')]
final class InvoiceController extends ResourceController
{
    protected function getEntityClass(): string
    {
        return Invoice::class;
    }

    protected function getResourceName(): string
    {
        return 'invoices';
    }

    #[Route('', name: 'store', methods: ['POST'])]
    public function store(Request $request): JsonResponse
    {
        $this->checkPermission('store');

        $data = $request->headers->get('Content-Type') === 'application/json'
            ? (json_decode($request->getContent(), true) ?? [])
            : $request->request->all();

        $invoice = new Invoice();
        $this->handleFileUploads($request, $data);
        $this->fillEntity($invoice, $data);

        // Custom validation using the inherited validator.
        $violations = $this->validator->validate($invoice, null, ['Default', 'store']);
        if (count($violations) > 0) {
            return $this->validationErrorResponse($violations);
        }

        $hookErrors = $this->runValidationHooks($invoice, $data, 'store');
        if (!empty($hookErrors)) {
            return $this->hookValidationErrorResponse($hookErrors);
        }

        // Fully custom persistence flow.
        $invoice->reference = $this->generateReference();
        $invoice->issuedAt = new \DateTimeImmutable();

        $this->entityManager->persist($invoice);
        $this->entityManager->flush();

        $this->saveChildRelations($invoice, $data);
        $this->entityManager->flush();

        $this->loadChildRelations($invoice);
        $this->loadRelationTitles($invoice);

        return JsonResponse::fromJsonString(
            $this->serializer->serialize($invoice, 'json'),
            Response::HTTP_CREATED
        );
    }

    private function generateReference(): string
    {
        return 'INV-' . strtoupper(bin2hex(random_bytes(4)));
    }
}
```

Notes:

- when overriding a route action, re-declare the `#[Route]` attribute so the
  route is still registered (mirror the inherited signature);
- keep `checkPermission()` and permission alignment with the resource actions;
- if you bypass `runValidationHooks()`, custom validation no longer runs —
  call it unless the override intentionally replaces it;
- do not duplicate behavior you can inherit; call the parent helpers.

---

## 7. Frontend validation hooks

Mirror every backend custom validation rule in the frontend for immediate
feedback.

Create the hook under:

```text
src/resources/hooks/<resourceName>/
```

With `write_file`, the frontend-relative path is:

```text
resources/hooks/<resourceName>/<hookName>.ts
```

Example:

```typescript
import type { ResourceValidationHook } from '../../../types/resource';

const orderValidationHook: ResourceValidationHook = {
  validate: (data, action, record) => {
    const errors: Record<string, string | string[]> = {};

    // mirror backend rules

    return errors;
  },
};

export default orderValidationHook;
```

The hook receives:

```text
(data, action, record)
```

where:

- `data` is the submitted form data;
- `action` is `"store"` or `"update"`;
- `record` is the existing record on update.

The frontend hook is for immediate feedback. The backend hook is authoritative.
They must implement the same logical rule.

Before-save and after-save lifecycle hooks are backend-only. The frontend has
no equivalent because they run server-side around persistence.

---

### 7.1 Validation registry safety

After creating a frontend validation hook, register it in:

```text
src/resources/hooks/index.ts
```

This registry may already contain hooks for other resources.

**Never overwrite it from memory.**

Required sequence:

1. Read the current registry.
2. Preserve all existing imports and registrations.
3. Add the new import/registration.
4. Overwrite the file with the full updated registry.

Tool sequence:

```javascript
read_file({
  shell: "frontend",
  path: "resources/hooks/index.ts"
})
```

Then:

```javascript
write_file({
  shell: "frontend",
  path: "resources/hooks/index.ts",
  content: "<full existing registry plus the new hook>"
})
```

Example final registry:

```typescript
import type { ResourceValidationHook } from '../../types/resource';
import userValidationHook from './users/userValidationHook';
import invoiceValidationHook from './invoices/invoiceValidationHook';
import leadValidationHook from './leads/leadValidationHook';

const validationHooks: Record<string, ResourceValidationHook[]> = {
  users: [userValidationHook],
  invoices: [invoiceValidationHook],
  leads: [leadValidationHook],
};

export default validationHooks;
```

Do not delete unrelated hook registrations.

When modifying an existing hook, read it if necessary, then write the complete
updated file. Keep backend and frontend mirrors synchronized.

---

## 8. Checklist

Before finishing hook or lifecycle work, verify:

1. Each hook file name matches its class name and lives under
   `Resource/Hooks/<resourceName>/` (backend) or
   `resources/hooks/<resourceName>/` (frontend).
2. `<resourceName>` matches `getResourceName()` exactly.
3. Backend and frontend validation rules are semantically identical.
4. Frontend validation hooks are registered without deleting existing entries.
5. After-save hooks explicitly persist and flush any additional changes.
6. Custom `store()`/`update()` overrides keep permission checks and re-declared
   routes, and reuse inherited helpers.
7. Every prompt requirement maps to exactly one chosen extension point (and no
   behavior is duplicated across layers).
