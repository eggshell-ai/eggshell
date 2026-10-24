'use client';

import React, { useEffect, useMemo, useState } from 'react';
import { Modal, Form, message } from 'antd';
import type { FieldConfig, ResourceActionForm } from '../../types/resource';
import FormField from '../resources/FormField';
import { processResourceFormValues } from '../resources/ResourceForm';
import apiService from '../../api/apiService';

interface ActionFormModalProps {
  /** Whether the dialog is visible */
  open: boolean;
  /** The form action configuration (field list, endpoint, labels, etc.) */
  config: ResourceActionForm | null;
  /** The record the action was triggered on (used to resolve `{id}`) */
  record: any;
  /** Called to close the dialog */
  onClose: () => void;
  /** Called after a successful submission */
  onSubmitted?: (result: any) => void;
  /** Optional resource name for validation hook resolution */
  resourceName?: string;
}

/** Resolve a field definition that may be a `Field` builder into its config. */
const resolveField = (field: any): FieldConfig =>
  typeof field?.config === 'function' ? field.config() : field;

/** Replace `{id}` in an endpoint template with the record id. */
const resolveEndpoint = (endpoint: string, record: any): string =>
  endpoint.replace(/\{id\}/g, String(record?.id ?? ''));

/**
 * A reusable modal dialog that renders a form from a declarative field list and
 * submits the collected values to a custom endpoint.
 *
 * This is the shared implementation behind resource "form" row actions. It
 * reuses the standard form engine (FormField) so all field types, validation,
 * and conditional visibility behave identically to normal create/edit forms,
 * with no bespoke React or CSS required.
 */
const ActionFormModal: React.FC<ActionFormModalProps> = ({
  open,
  config,
  record,
  onClose,
  onSubmitted,
  resourceName,
}) => {
  const [form] = Form.useForm();
  const [submitting, setSubmitting] = useState(false);

  const fields = useMemo<FieldConfig[]>(
    () => (config?.fields ?? []).map(resolveField),
    [config]
  );

  useEffect(() => {
    if (open) {
      form.resetFields();
      if (config?.values) {
        form.setFieldsValue(config.values);
      }
    }
  }, [open, config, form]);

  const handleSubmit = async () => {
    setSubmitting(true);
    try {
      const values = await form.validateFields();
      const payload = processResourceFormValues(values, fields);

      const endpoint = config?.endpoint
        ? resolveEndpoint(config.endpoint, record)
        : `/${resourceName}/${record?.id}`;

      const method = (config?.method || 'POST').toUpperCase();
      let result: any;
      if (method === 'PUT') {
        result = await apiService.put(endpoint, payload);
      } else if (method === 'PATCH') {
        result = await apiService.patch(endpoint, payload);
      } else {
        result = await apiService.post(endpoint, payload);
      }

      message.success(config?.successMessage || 'Saved successfully');
      onClose();
      onSubmitted?.(result);
    } catch (error: any) {
      if (error?.errorFields) {
        return; // form validation errors are shown inline
      }
      const errorMsg =
        error?.response?.data?.detail ||
        error?.response?.data?.message ||
        error?.response?.data?.error ||
        'Action failed';
      message.error(errorMsg);
      console.error('Error submitting action form:', error);
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Modal
      title={config?.title || 'Action'}
      open={open}
      onOk={handleSubmit}
      onCancel={onClose}
      okText={config?.submitLabel || 'Submit'}
      cancelText="Cancel"
      confirmLoading={submitting}
      destroyOnClose
      width="60%"
    >
      <Form form={form} layout="vertical">
        {fields.map((field, index) => (
          <FormField
            key={field.name || index}
            field={field}
            form={form}
            resourceName={resourceName}
            action="store"
          />
        ))}
      </Form>
    </Modal>
  );
};

export default ActionFormModal;
