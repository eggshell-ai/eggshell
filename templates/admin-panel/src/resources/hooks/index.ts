import type { ResourceValidationHook } from '../../types/resource';
import userValidationHook from './users/userValidationHook';

/**
 * Resource validation hook registry.
 *
 * Mirrors the backend's automatic hook discovery (ResourceController::getHooks):
 * hooks live in src/resources/hooks/<resourceName>/ and are registered here.
 *
 * This file contains ONLY the registry so it can be cheaply read and
 * overwritten (read_file then write_file) whenever a new hook is added.
 * The hook runner lives in src/utils/resourceValidationHooks.ts.
 */
const validationHooks: Record<string, ResourceValidationHook[]> = {
  users: [userValidationHook],
};

export default validationHooks;
