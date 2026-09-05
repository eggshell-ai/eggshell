import { z } from 'zod';
import type { FieldConfig } from '../types/resource';
import { runValidationHooks } from './resourceValidationHooks';

/**
 * Validation engine
 * Converts a field's `validations` config into a zod schema and
 * validates the field value in the context of the entire data object
 * (to facilitate cross-validation later).
 */

/**
 * Map field type to a base zod schema
 */
const buildBaseSchema = (field: FieldConfig): z.ZodTypeAny => {
  const isRequired = Boolean(field.validations?.required);

  switch (field.type) {
    case 'number':
      return isRequired ? z.number({ invalid_type_error: ' ' }) : z.number().optional();
    case 'boolean':
      return isRequired ? z.boolean() : z.boolean().optional();
    case 'date':
      return isRequired ? z.any() : z.any().optional();
    default:
      return isRequired ? z.string({ invalid_type_error: ' ' }) : z.string().optional();
  }
};

/**
 * Apply the validations config (required, length, etc.) to a base schema
 */
export const buildFieldSchema = (field: FieldConfig): z.ZodTypeAny => {
  let schema = buildBaseSchema(field);
  const validations = field.validations || {};
  const messages = field.messages || {};

  if (validations.required) {
    // required marker — handled via refine for emptiness
    schema = (schema as any).refine((val: any) => {
      if (val === undefined || val === null) return false;
      if (typeof val === 'string') return val.trim().length > 0;
      if (Array.isArray(val)) return val.length > 0;
      return true;
    }, { message: messages.required || `Please enter ${field.label || field.name}` });
  }

  if (validations.length && field.type !== 'number') {
    const { min, max } = validations.length;
    let strSchema = z.string();
    if (min !== undefined) {
      strSchema = strSchema.min(min, { message: messages.minLength || `${field.label || field.name} must be at least ${min} characters` });
    }
    if (max !== undefined) {
      strSchema = strSchema.max(max, { message: messages.maxLength || `${field.label || field.name} must be at most ${max} characters` });
    }
    if (!validations.required) {
      strSchema = strSchema.optional().or(z.literal('')) as any;
    }
    schema = strSchema;
  }

  if (validations.length && field.type === 'number') {
    const { min, max } = validations.length;
    let numSchema = z.number({ invalid_type_error: messages.number || `Please enter a valid ${field.label || field.name}` });
    if (min !== undefined) {
      numSchema = numSchema.min(min, { message: messages.min || `${field.label || field.name} must be at least ${min}` });
    }
    if (max !== undefined) {
      numSchema = numSchema.max(max, { message: messages.max || `${field.label || field.name} must be at most ${max}` });
    }
    if (!validations.required) {
      numSchema = numSchema.optional() as any;
    }
    schema = numSchema;
  }

  // Built-in type rules
  if (field.type === 'email' || validations.email) {
    let emailSchema = z.string().email({ message: messages.email || 'Please enter a valid email' });
    if (!validations.required) {
      emailSchema = emailSchema.optional().or(z.literal('')) as any;
    }
    schema = emailSchema;
  }

  // Phone (E.164) validation
  if (field.type === 'phone' || validations.phone) {
    let phoneSchema = z.string().regex(/^\+[1-9]\d{1,14}$/, {
      message: messages.phone || `Please enter a valid phone number (E.164 format, e.g. +14155552671)`,
    });
    if (!validations.required) {
      phoneSchema = phoneSchema.optional().or(z.literal('')) as any;
    }
    schema = phoneSchema;
  }

  // Static select validation — value must be one of the configured options (or empty if not required)
  if (field.options && Object.keys(field.options).length > 0) {
    const allowedValues = Object.keys(field.options);
    let optionsSchema = z.string().refine(
      (val) => allowedValues.includes(val),
      { message: messages.options || `${field.label || field.name} must be one of: ${allowedValues.map((v) => field.options![v]).join(', ')}` }
    );
    if (!validations.required) {
      optionsSchema = optionsSchema.optional().or(z.literal('')).or(z.literal(undefined)) as any;
    }
    schema = optionsSchema;
  }

  return schema;
};

export interface ValidationResult {
  valid: boolean;
  error?: string;
}

/**
 * Validate a single field value in the context of the entire form data.
 * Cross-field validation can be added later using `allValues`.
 */
export const validateField = (
  field: FieldConfig,
  value: any,
  allValues: any = {}
): ValidationResult => {
  const schema = buildFieldSchema(field);
  const result = schema.safeParse(value !== undefined ? value : undefined);
  if (result.success) {
    return { valid: true };
  }
  return {
    valid: false,
    error: result.error.issues[0]?.message || `Invalid value for ${field.label || field.name}`,
  };
};

/**
 * Validate the entire data object against a set of fields.
 * Returns a map of field name -> error message.
 */
export const validateAll = (fields: FieldConfig[], values: any): Record<string, string> => {
  const errors: Record<string, string> = {};
  fields.forEach((field) => {
    const result = validateField(field, values?.[field.name], values);
    if (!result.valid && result.error) {
      errors[field.name] = result.error;
    }
  });
  return errors;
};

/**
 * Run resource validation hooks (mirroring the backend's ValidatesResource hooks)
 * and normalize their errors into a map of field name -> first error message.
 *
 * @param resourceName Name of the resource whose hooks should run (e.g. 'users')
 * @param action       Either 'store' or 'update'
 * @param record       The existing record being updated (undefined on store)
 */
export const validateWithHooks = async (
  resourceName: string,
  values: any,
  action: 'store' | 'update',
  record?: any
): Promise<Record<string, string>> => {
  const hookErrors = await runValidationHooks(resourceName, values, action, record);
  const errors: Record<string, string> = {};
  Object.entries(hookErrors).forEach(([field, messages]) => {
    if (messages.length > 0) {
      errors[field] = messages[0];
    }
  });
  return errors;
};
