# Custom Validation

---

## 1. When to use custom validation

Use declarative field config for single-field rules (required, unique, size,
format). Use a custom validation hook when the rule is cross-field or
conditional and cannot be represented declaratively.

Custom validation runs during `store` and `update`, **after** standard
declarative validation. Backend errors are returned in the normal validation
response and displayed by `ResourcePage` beside the relevant field.

---

## 2. Backend validation hook

For validation that cannot be represented declaratively, implement
`ValidatesResource` in a hook class under `Resource/Hooks/<resourceName>/`.

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

Return:
```php
[
    'fieldName' => 'Error message'
]
```
or arrays of messages if needed. Return an empty array when validation passes.

Keep errors field-addressable so they display beside the relevant field.

Hook classes are auto-discovered: one class per file, class name must match file
name, namespace `App\Resource\Hooks\<resourceName>`. The `<resourceName>` segment
must match `getResourceName()` exactly. Hook classes are instantiated with no
constructor arguments.

## 3. Frontend Validation Hook
Backend remains the authoritive source for validations, but frontend hooks can be added to mirror the backend and flag errors immediately before submission.

Create `resources/hooks/<resource>/<validationHookName>.ts`:

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

### Step 4 — Register the frontend hook

Read the current registry, preserve existing entries, add the new one:

```javascript
read_file({ shell: "frontend", path: "resources/hooks/index.ts" })
```

Then add the new hook using either `write_file` or `patch_file`:

```javascript
write_file({
  shell: "frontend",
  path: "resources/hooks/index.ts",
  content: "<full existing registry plus the new hook>"
})
```

Never overwrite the registry from memory. Do not delete unrelated hook
registrations. Example final registry:

```typescript
import type { ResourceValidationHook } from '../../types/resource';
import userValidationHook from './users/userValidationHook';
import invoiceValidationHook from './invoices/invoiceValidationHook';
// Import from the file we created, we are already in `resources` directory so no need to write it again
import leadValidationHook from './leads/leadValidationHook';

const validationHooks: Record<string, ResourceValidationHook[]> = {
  users: [userValidationHook],
  invoices: [invoiceValidationHook],
  leads: [leadValidationHook], // Register the new hook
};

export default validationHooks;
```

# Custom Validation Messages
It is possible to override messages for built-in validations. Only do this if the user explicitly asks you to do that.
It must be done per-field in `sync_schema`

```json
{
  name: "email",
  type: "email",
  required: true,
  unique: true,
  messages: {
    required: "Email is required.",
    email: "Enter a valid email address.",
    unique: "This email address is already in use."
  }
}
```