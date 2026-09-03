'use client';

import React, { useState } from 'react';
import { Form, Button, Space } from 'antd';
import { FilterOutlined, SearchOutlined } from '@ant-design/icons';
import type { Resource } from '../../types/resource';
import FormField from './FormField';
import FilterField from './FilterField';

interface ResourceFormProps {
  resource: Resource;
  form: any;
  initialValues?: any;
  excludeFields?: string[];
  onFinish?: (values: any) => void | Promise<void>;
}

interface ResourceFilterFormProps {
  resource: Resource;
  filters: Record<string, any> | null;
  onApply: (filters: Record<string, any> | null) => void;
}

/**
 * Filtration form shown when the filter button is clicked.
 * Only renders fields marked as `filterable: true`.
 */
const ResourceFilterForm: React.FC<ResourceFilterFormProps> = ({
  resource,
  filters,
  onApply,
}) => {
  const [form] = Form.useForm();

  const filterFields = resource.fields.filter(
    (field) => field.filterable === true
  );

  React.useEffect(() => {
    if (filters) {
      form.setFieldsValue(filters);
    } else {
      form.resetFields();
    }
  }, [filters]);

  const handleOk = () => {
    const values = form.getFieldsValue();
    const payload = buildFilterPayload(values, filterFields);
    onApply(Object.keys(payload).length > 0 ? payload : null);
  };

  const handleReset = () => {
    form.resetFields();
    onApply(null);
  };

  return (
    <div
      style={{
        border: '1px solid #f0f0f0',
        borderRadius: 8,
        padding: 16,
        marginBottom: 16,
        backgroundColor: '#fafafa',
      }}
    >
      <Form form={form} layout="vertical">
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(240px, 1fr))', gap: '0 16px' }}>
          {filterFields.map((field) => (
            <FilterField key={field.name} field={field} />
          ))}
        </div>
        <Space style={{ marginTop: 8 }}>
          <Button type="primary" icon={<SearchOutlined />} onClick={handleOk}>
            OK
          </Button>
          <Button onClick={handleReset}>Reset</Button>
        </Space>
      </Form>
    </div>
  );
};

/**
 * Converts raw filter form values into a query payload for the server.
 *
 * - Text fields with "Is Empty" checked produce `{ __isEmpty: true }`
 * - Boolean fields with "yes"/"no" produce `true`/`false`; "any" is skipped
 * - Empty/undefined values are dropped
 */
export const buildFilterPayload = (
  values: Record<string, any>,
  fields: any[]
): Record<string, any> => {
  const payload: Record<string, any> = {};

  fields.forEach((field) => {
    const isEmpty = values[`${field.name}__isEmpty`] === true;

    if (isEmpty) {
      payload[field.name] = { __isEmpty: true };
      return;
    }

    const value = values[field.name];
    if (value === undefined || value === null || value === '') {
      return;
    }

    if (field.type === 'boolean') {
      if (value === 'yes') {
        payload[field.name] = true;
      } else if (value === 'no') {
        payload[field.name] = false;
      }
      // 'any' or anything else means no filtering
      return;
    }

    payload[field.name] = value;
  });

  return payload;
};

const ResourceForm: React.FC<ResourceFormProps> = ({
  resource,
  form,
  initialValues,
  excludeFields = [],
  onFinish,
}) => {
  // Filter fields that should be shown in form
  const formFields = resource.fields.filter(
    (field) =>
      (field.form !== false) && // Show if form is not explicitly false
      !excludeFields.includes(field.name) // Exclude specified fields
  );

  // Check if form has file fields
  const hasFileFields = formFields.some((field) => field.type === 'file');

  const handleFinish = (values: any) => {
    if (onFinish) {
      const payload = processResourceFormValues(values, formFields);
      onFinish(payload);
    }
  };

  return (
    <Form
      form={form}
      layout="vertical"
      initialValues={initialValues}
      onFinish={handleFinish}
    >
      {formFields.map((field) => (
        <FormField key={field.name} field={field} form={form} />
      ))}
    </Form>
  );
};

export const processResourceFormValues = (values: any, fields: any[]) => {
  const formFields = fields.filter((field) => field.form !== false);
  const hasFileFields = formFields.some((field) => field.type === 'file');

  if (!hasFileFields) {
    return values;
  }

  const formData = new FormData();

  Object.keys(values).forEach((key) => {
    const field = formFields.find((f) => f.name === key);
    const value = values[key];

    if (field?.type === 'file') {
      if (value) {
        let file: File | null = null;

        if (Array.isArray(value) && value.length > 0) {
          file = value[0]?.originFileObj || (value[0] instanceof File ? value[0] : null);
        } else if (value.fileList && Array.isArray(value.fileList) && value.fileList.length > 0) {
          file = value.fileList[0]?.originFileObj || (value.fileList[0] instanceof File ? value.fileList[0] : null);
        } else if (value.originFileObj) {
          file = value.originFileObj;
        } else if (value instanceof File) {
          file = value;
        }

        if (file) {
          formData.append(key, file);
        } else if (typeof value === 'string') {
          formData.append(key, value);
        }
      }
    } else if (value !== undefined && value !== null) {
      if (typeof value === 'object') {
        formData.append(key, JSON.stringify(value));
      } else {
        formData.append(key, String(value));
      }
    }
  });

  return formData;
};

export default ResourceForm;

export { ResourceFilterForm };
