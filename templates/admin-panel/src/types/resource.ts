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
  filterable?: boolean;
  required?: boolean;
  columns?: any[];
  map?: string;
  minSize?: number;
  maxSize?: number;
  readonly?: boolean;
  compute?: (values: any) => any;
  default?: any;
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
