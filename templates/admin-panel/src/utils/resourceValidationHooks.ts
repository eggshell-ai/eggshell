import validationHooks from '../resources/hooks';
import type { ResourceValidationHook } from '../types/resource';

/**
 * Resource validation hook runner.
 *
 * Mirrors the backend's ResourceController::getHooks / runValidationHooks.
 * The registry itself lives in src/resources/hooks/index.ts and contains
 * only the hook map so it can be cheaply read and overwritten.
 */

const hookCache: Record<string, ResourceValidationHook[]> = {};

/**
 * Get all validation hooks registered for a resource.
 * Results are cached per resource name, like the backend implementation.
 */
export const getValidationHooks = (resourceName: string): ResourceValidationHook[] => {
  if (resourceName in hookCache) {
    return hookCache[resourceName];
  }
  const hooks = validationHooks[resourceName] || [];
  hookCache[resourceName] = hooks;
  return hooks;
};

/**
 * Run all validation hooks for a resource and collect their errors.
 *
 * Mirrors the backend's runValidationHooks: each hook may return a map of
 * field name -> message(s); errors from all hooks are merged together.
 *
 * @returns Record<fieldName, string[]> — empty when validation passes.
 */
export const runValidationHooks = async (
  resourceName: string,
  data: any,
  action: 'store' | 'update',
  record?: any
): Promise<Record<string, string[]>> => {
  const errors: Record<string, string[]> = {};

  for (const hook of getValidationHooks(resourceName)) {
    let hookErrors;
    try {
      hookErrors = await hook.validate(data, action, record);
    } catch (e) {
      console.error(`[validationHooks] Error running hook for "${resourceName}":`, e);
      continue;
    }

    if (!hookErrors) {
      continue;
    }

    Object.entries(hookErrors).forEach(([field, messages]) => {
      const list = Array.isArray(messages) ? messages : [messages];
      errors[field] = [...(errors[field] || []), ...list];
    });
  }

  return errors;
};
