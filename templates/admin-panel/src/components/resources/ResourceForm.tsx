'use client';

import React from 'react';
import { Form } from 'antd';
import type { Resource } from '../../types/resource';
import FormField from './FormField';

interface ResourceFormProps {
  resource: Resource;
  form: any;
  initialValues?: any;
  excludeFields?: string[];
  onFinish?: (values: any) => void | Promise<void>;
}

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
