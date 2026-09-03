import { z } from 'zod';
import type { FieldConfig } from '../types/resource';

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

  if (validations.required) {
    // required marker — handled via refine for emptiness
    schema = (schema as any).refine((val: any) => {
      if (val === undefined || val === null) return false;
      if (typeof val === 'string') return val.trim().length > 0;
      if (Array.isArray(val)) return val.length > 0;
      return true;
    }, { message: `Please enter ${field.label || field.name}` });
  }

  if (validations.length && field.type !== 'number') {
    const { min, max } = validations.length;
    let strSchema = z.string();
    if (min !== undefined) {
      strSchema = strSchema.min(min, { message: `${field.label || field.name} must be at least ${min} characters` });
    }
    if (max !== undefined) {
      strSchema = strSchema.max(max, { message: `${field.label || field.name} must be at most ${max} characters` });
    }
    if (!validations.required) {
      strSchema = strSchema.optional().or(z.literal('')) as any;
    }
    schema = strSchema;
  }

  if (validations.length && field.type === 'number') {
    const { min, max } = validations.length;
    let numSchema = z.number({ invalid_type_error: `Please enter a valid ${field.label || field.name}` });
    if (min !== undefined) {
      numSchema = numSchema.min(min, { message: `${field.label || field.name} must be at least ${min}` });
    }
    if (max !== undefined) {
      numSchema = numSchema.max(max, { message: `${field.label || field.name} must be at most ${max}` });
    }
    if (!validations.required) {
      numSchema = numSchema.optional() as any;
    }
    schema = numSchema;
  }

  // Built-in type rules
  if (field.type === 'email') {
    let emailSchema = z.string().email({ message: 'Please enter a valid email' });
    if (!validations.required) {
      emailSchema = emailSchema.optional().or(z.literal('')) as any;
    }
    schema = emailSchema;
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
