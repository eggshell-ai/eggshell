export interface FieldMessages {
  required?: string;
  email?: string;
  phone?: string;
  unique?: string;
  minLength?: string;
  maxLength?: string;
  min?: string;
  max?: string;
  [key: string]: string | undefined;
}

/**
 * Field configuration interface
 */
export interface FieldConfig {
  type: string;
  name: string;
  label?: string;
  resource?: Resource | any;
  table?: boolean;
  form?: boolean;
  detail?: boolean;
  filterable?: boolean;
  required?: boolean;
  columns?: any[];
  map?: string;
  minSize?: number;
  maxSize?: number;
  readonly?: boolean;
  compute?: (values: any) => any;
  default?: any;
  messages?: FieldMessages;
  trueLabel?: string;
  falseLabel?: string;
  /** Static options for select fields: keys are values, values are labels */
  options?: Record<string, string>;
  [key: string]: any;
}

/**
 * Field interface with chainable methods
 */
export interface Field {
  config(): FieldConfig;
  label(label: string): Field;
  resource(resource: Resource | any): Field;
  table(): Field;
  form(): Field;
  detail(): Field;
  filterable(): Field;
  required(): Field;
  columns(columns: any[]): Field;
  map(map: string): Field;
  minSize(min: number): Field;
  maxSize(max: number): Field;
  phone(): Field;
  readonly(isReadonly?: boolean): Field;
  compute(fn: (values: any) => any): Field;
  default(value: any): Field;
  messages(messages: FieldMessages): Field;
  trueLabel(label: string): Field;
  falseLabel(label: string): Field;
  options(options: Record<string, string>): Field;
}

/**
 * Validation errors returned by resource validation hooks.
 * Maps a field name to one or more error messages.
 */
export type ResourceValidationErrors = Record<string, string | string[]>;

/**
 * Validation hook interface (frontend mirror of the backend's ValidatesResource).
 *
 * Implement this interface in a hook file placed under
 * src/resources/hooks/<resourceName>/ and register it in
 * src/resources/hooks/index.ts to have it run automatically
 * during store and update operations, after standard field validation.
 */
export interface ResourceValidationHook {
  validate: (
    /** Raw form values about to be submitted */
    data: any,
    /** Either 'store' or 'update' */
    action: 'store' | 'update',
    /** The existing record being updated (undefined on store) */
    record?: any
  ) => ResourceValidationErrors | void | Promise<ResourceValidationErrors | void>;
}

/**
 * Resource interface
 */
export interface Resource<T = any> {
  name: string;
  service: CrudService<T>;
  fields: FieldConfig[];
  permissions: {
    create: string;
    edit: string;
    view: string;
    delete: string;
  };
}

/**
 * CrudService interface
 */
export interface CrudService<T = any> {
  list(config?: any): Promise<T[]>;
  query(config?: any): Promise<T[]>;
  get(id: string | number, config?: any): Promise<T>;
  create(data: Partial<T>, config?: any): Promise<T>;
  update(id: string | number, data: Partial<T>, config?: any): Promise<T>;
  delete(id: string | number, config?: any): Promise<void>;
}

export default Resource;
