import type { ResourceValidationHook } from '../../../types/resource';

/**
 * Example validation hook for the "users" resource.
 *
 * File: src/resources/hooks/users/userValidationHook.ts
 * Mirrors the backend hook: src/Resource/Hooks/users/UserValidationHook.php
 *
 * When modifying this file, read it first (read_file), then overwrite
 * it entirely with the full updated content (write_file).
 */
const userValidationHook: ResourceValidationHook = {
  validate: (data, action) => {
    const errors: Record<string, string | string[]> = {};

    // Example: enforce a minimum password length on creation
    if (
      action === 'store' &&
      data?.password !== undefined &&
      data?.password !== null &&
      String(data.password).length > 0 &&
      String(data.password).length < 8
    ) {
      errors.password = 'Password must be at least 8 characters long.';
    }

    return errors;
  },
};

export default userValidationHook;
