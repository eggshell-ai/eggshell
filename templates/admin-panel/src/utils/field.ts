import { Field, FieldConfig } from '../types/resource';

/**
 * Field builder class
 */
class FieldBuilder implements Field {
  private _config: FieldConfig;

  constructor(type: string, name: string) {
    this._config = {
      type,
      name,
    };
  }

  config(): FieldConfig {
    return { ...this._config };
  }

  label(label: string): Field {
    this._config.label = label;
    return this;
  }

  resource(resource: any): Field {
    this._config.resource = resource;
    return this;
  }

  table(): Field {
    this._config.table = true;
    return this;
  }

  form(): Field {
    this._config.form = true;
    return this;
  }

  detail(): Field {
    this._config.detail = true;
    return this;
  }

  filterable(): Field {
    this._config.filterable = true;
    return this;
  }

  required(): Field {
    this._config.validations = {
      ...this._config.validations,
      required: {},
    };
    return this;
  }

  minSize(min: number): Field {
    this._config.validations = {
      ...this._config.validations,
      minSize: { min },
    };
    return this;
  }

  maxSize(max: number): Field {
    this._config.validations = {
      ...this._config.validations,
      maxSize: { max },
    };
    return this;
  }

  email(): Field {
    this._config.validations = {
      ...this._config.validations,
      email: {},
    };
    return this;
  }

  phone(): Field {
    this._config.type = 'phone';
    this._config.validations = {
      ...this._config.validations,
      phone: {},
    };
    return this;
  }

  searchable(): Field {
    this._config.searchable = true;
    return this;
  }

  sortable(): Field {
    this._config.sortable = true;
    return this;
  }

  unique(): Field {
    this._config.unique = true;
    return this;
  }

  accept(accept: string): Field {
    this._config.accept = accept;
    return this;
  }

  columns(columns: any[]): Field {
    this._config.columns = columns;
    return this;
  }

  map(map: string): Field {
    this._config.map = map;
    return this;
  }

  readonly(isReadonly: boolean = true): Field {
    this._config.readonly = isReadonly;
    return this;
  }

  compute(fn: (values: any) => any): Field {
    this._config.compute = fn;
    return this;
  }

  default(value: any): Field {
    this._config.default = value;
    return this;
  }

  trueLabel(label: string): Field {
    this._config.trueLabel = label;
    return this;
  }

  falseLabel(label: string): Field {
    this._config.falseLabel = label;
    return this;
  }
}

/**
 * Field utility function
 * Creates field builders for different types
 */
export const field = {
  text(name: string): Field {
    return new FieldBuilder('text', name);
  },

  email(name: string): Field {
    return new FieldBuilder('email', name);
  },

  phone(name: string): Field {
    const builder = new FieldBuilder('phone', name);
    builder.phone();
    return builder;
  },

  select(name: string): Field {
    return new FieldBuilder('select', name);
  },

  tags(name: string): Field {
    return new FieldBuilder('tags', name);
  },

  number(name: string): Field {
    return new FieldBuilder('number', name);
  },

  date(name: string): Field {
    return new FieldBuilder('date', name);
  },

  boolean(name: string): Field {
    return new FieldBuilder('boolean', name);
  },

  textarea(name: string): Field {
    return new FieldBuilder('textarea', name);
  },

  password(name: string): Field {
    return new FieldBuilder('password', name);
  },

  file(name: string): Field {
    return new FieldBuilder('file', name);
  },

  table(name: string): Field {
    return new FieldBuilder('table', name);
  },

  foreign(name: string): Field {
    return new FieldBuilder('foreign', name);
  },
};

export default field;
