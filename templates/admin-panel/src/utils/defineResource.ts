import { Resource, FieldConfig, Field } from '../types/resource';
import { createCrudService } from '../api/createCrudService';

/**
 * defineResource utility function
 * Converts the given object and returns the resource
 * If endpoint is provided instead of service, a CRUD service will be auto-generated
 */
export function defineResource<T = any>(config: {
  name: string;
  service?: any;
  endpoint?: string;
  fields: Field[];
  permissions: {
    create: string,
    edit: string,
    view: string,
    delete: string,
  }
}): Resource<T> {
  const service = config.service || (config.endpoint ? createCrudService<T>(config.endpoint) : undefined);
  
  if (!service) {
    throw new Error('Either service or endpoint must be provided');
  }

  return {
    name: config.name,
    service,
    fields: config.fields.map(field => {
      const fieldConfig = field.config();
      if (typeof fieldConfig.resource === 'string') {
        const resourceName = fieldConfig.resource;
        let resolvedResource: any = null;
        Object.defineProperty(fieldConfig, 'resource', {
          get() {
            if (!resolvedResource) {
              try {
                const resModule = require(`../resources/${resourceName}`);
                resolvedResource = resModule.default || resModule;
              } catch (error) {
                console.error(`[defineResource] Could not resolve resource "${resourceName}":`, error);
              }
            }
            return resolvedResource;
          },
          configurable: true,
          enumerable: true,
        });
      }
      return fieldConfig;
    }),
    permissions: config.permissions
  };
}

export default defineResource;
