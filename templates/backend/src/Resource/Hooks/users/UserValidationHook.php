<?php

namespace App\Resource\Hooks\users;

use App\Resource\Hook\ValidatesResource;

/**
 * Example validation hook for the "users" resource.
 *
 * File: src/Resource/Hooks/users/UserValidationHook.php
 * Class name must match the file name exactly.
 *
 * A matching frontend hook mirrors this validation for immediate
 * feedback in the form:
 * File: src/resources/hooks/users/userValidationHook.ts
 *
 * When modifying this file, read it first (read_file), then overwrite
 * it entirely with the full updated content (write_file).
 */
class UserValidationHook implements ValidatesResource
{
    public function validate(object $entity, array $data, string $action): array
    {
        $errors = [];

        // Example: enforce a minimum password length on creation
        if ($action === 'store' && isset($data['password']) && strlen((string) $data['password']) < 8) {
            $errors['password'] = 'Password must be at least 8 characters long.';
        }

        return $errors;
    }
}
