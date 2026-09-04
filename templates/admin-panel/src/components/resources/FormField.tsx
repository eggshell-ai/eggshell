'use client';

import React, { useState, useEffect } from 'react';
import { Form, Input as AntInput, Select, InputNumber, DatePicker, Checkbox, Upload, message, Button } from 'antd';
import { PlusOutlined, DeleteOutlined } from '@ant-design/icons';
import dayjs from 'dayjs';
import type { FieldConfig } from '../../types/resource';
import { validateField } from '../../utils/validation';
import apiService from '../../api/apiService';

interface FormFieldProps {
  field: FieldConfig;
  form: any;
  initialValues?: any;
  name?: string | number | (string | number)[];
  noLabel?: boolean;
  parentName?: string;
}

const formatColumnLabel = (name: string): string => {
  if (!name) return '';
  return name
    .replace(/_/g, ' ')
    .replace(/([a-z])([A-Z])/g, '$1 $2')
    .replace(/\b\w/g, (char) => char.toUpperCase());
};

interface TableColumnParsed {
  name: string;
  label: string;
  value?: (data: any) => any;
  isCalculated: boolean;
  isHidden: boolean;
  colFieldConfig: FieldConfig;
}

const CalculatedCell: React.FC<{
  form: any;
  tableName: string;
  rowIndex: number;
  col: TableColumnParsed;
}> = ({ form, tableName, rowIndex, col }) => {
  const rowData = Form.useWatch([tableName, rowIndex], form);

  let calculatedValue: any = '';
  if (col.value && typeof col.value === 'function') {
    try {
      calculatedValue = col.value(rowData || {});
      if (Number.isNaN(calculatedValue) || calculatedValue === undefined || calculatedValue === null) {
        calculatedValue = '';
      }
    } catch (e) {
      calculatedValue = '';
    }
  }

  useEffect(() => {
    if (calculatedValue !== '') {
      const currentValue = form.getFieldValue([tableName, rowIndex, col.name]);
      if (currentValue !== calculatedValue) {
        form.setFieldValue([tableName, rowIndex, col.name], calculatedValue);
      }
    }
  }, [calculatedValue, form, tableName, rowIndex, col.name]);

  return (
    <div
      style={{
        padding: '5px 12px',
        backgroundColor: '#fafafa',
        borderRadius: '6px',
        border: '1px solid #d9d9d9',
        minHeight: '32px',
        display: 'flex',
        alignItems: 'center',
        fontWeight: 500,
        color: '#262626',
      }}
    >
      <Form.Item name={[rowIndex, col.name]} hidden style={{ display: 'none' }}>
        <AntInput type="hidden" />
      </Form.Item>
      {calculatedValue !== '' ? calculatedValue : '-'}
    </div>
  );
};

const TableWidget: React.FC<{
  field: FieldConfig;
  form: any;
}> = ({ field, form }) => {
  const rawColumns: any[] = field.columns || [];

  // Extract fields from selected resource if available
  const resourceFields: FieldConfig[] =
    field.resource && typeof field.resource === 'object' && Array.isArray(field.resource.fields)
      ? field.resource.fields.map((f: any) => (typeof f.config === 'function' ? f.config() : f))
      : [];

  // Parse columns
  const parsedColumns: TableColumnParsed[] = rawColumns.map((col: any) => {
    const colName = typeof col === 'string' ? col : col.name;
    const colValueFn = typeof col === 'object' ? col.value : undefined;
    const colExplicitLabel = typeof col === 'object' ? col.label : undefined;

    // Look up in resource fields
    const resField = resourceFields.find((f: FieldConfig) => f.name === colName);

    const label = colExplicitLabel || resField?.label || formatColumnLabel(colName);
    const isHidden = colName.toLowerCase() === 'id' || resField?.type === 'hidden';
    const isCalculated = typeof colValueFn === 'function';

    const colFieldConfig: FieldConfig = {
      type: resField?.type || 'text',
      name: colName,
      label: label,
      ...(resField || {}),
      ...(typeof col === 'object' ? col : {}),
    };

    return {
      name: colName,
      label,
      value: colValueFn,
      isCalculated,
      isHidden,
      colFieldConfig,
    };
  });

  const visibleColumns = parsedColumns.filter((c) => !c.isHidden);
  const hiddenColumns = parsedColumns.filter((c) => c.isHidden);

  // Build default values object for new table rows from column field configs
  const rowDefaults = (resourceFields || []).reduce((acc: Record<string, any>, f: FieldConfig) => {
    if (f.default !== undefined) {
      acc[f.name] = f.default;
    }
    return acc;
  }, {});
  const hasRowDefaults = Object.keys(rowDefaults).length > 0;

  return (
    <div style={{ marginBottom: 24 }}>
      {field.label && (
        <div style={{ fontWeight: 600, fontSize: 14, marginBottom: 8, color: '#1f1f1f' }}>
          {field.label}
        </div>
      )}
      <Form.List name={field.name}>
        {(fields, { add, remove }) => (
          <div
            style={{
              border: '1px solid #f0f0f0',
              borderRadius: '8px',
              overflow: 'hidden',
              backgroundColor: '#fff',
            }}
          >
            <table
              style={{
                width: '100%',
                borderCollapse: 'collapse',
                textAlign: 'left',
                fontSize: 14,
              }}
            >
              <thead>
                <tr style={{ backgroundColor: '#fafafa', borderBottom: '1px solid #f0f0f0' }}>
                  <th style={{ padding: '10px 12px', width: 48, textAlign: 'center', color: '#595959', fontWeight: 600 }}>#</th>
                  {visibleColumns.map((col) => (
                    <th key={col.name} style={{ padding: '10px 12px', color: '#595959', fontWeight: 600 }}>
                      {col.label}
                    </th>
                  ))}
                  <th style={{ padding: '10px 12px', width: 60, textAlign: 'center', color: '#595959', fontWeight: 600 }}>Action</th>
                </tr>
              </thead>
              <tbody>
                {fields.length === 0 ? (
                  <tr>
                    <td
                      colSpan={visibleColumns.length + 2}
                      style={{
                        textAlign: 'center',
                        padding: '24px 12px',
                        color: '#8c8c8c',
                      }}
                    >
                      No items added yet.
                    </td>
                  </tr>
                ) : (
                  fields.map((rowField, index) => (
                    <tr key={rowField.key} style={{ borderBottom: '1px solid #f0f0f0' }}>
                      <td style={{ padding: '8px 12px', textAlign: 'center', color: '#8c8c8c', verticalAlign: 'top' }}>
                        {index + 1}
                      </td>

                      {/* Hidden fields like ID */}
                      {hiddenColumns.map((hCol) => (
                        <td key={hCol.name} style={{ display: 'none' }}>
                          <Form.Item name={[rowField.name, hCol.name]} hidden style={{ display: 'none' }}>
                            <AntInput type="hidden" />
                          </Form.Item>
                        </td>
                      ))}

                      {/* Visible Column Cells */}
                      {visibleColumns.map((col) => (
                        <td key={col.name} style={{ padding: '8px 12px', verticalAlign: 'top' }}>
                          {col.isCalculated ? (
                            <CalculatedCell
                              form={form}
                              tableName={field.name}
                              rowIndex={rowField.name}
                              col={col}
                            />
                          ) : (
                            <FormField
                              field={col.colFieldConfig}
                              form={form}
                              name={[rowField.name, col.name]}
                              parentName={field.name}
                              noLabel
                            />
                          )}
                        </td>
                      ))}

                      {/* Action Cell */}
                      <td style={{ padding: '8px 12px', textAlign: 'center', verticalAlign: 'top' }}>
                        <Button
                          type="text"
                          danger
                          icon={<DeleteOutlined />}
                          onClick={() => remove(rowField.name)}
                        />
                      </td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
            <div style={{ padding: '8px 12px', borderTop: '1px solid #f0f0f0', backgroundColor: '#fafafa' }}>
              <Button
                type="dashed"
                onClick={() => add(hasRowDefaults ? rowDefaults : undefined)}
                icon={<PlusOutlined />}
                block
              >
                Add Item
              </Button>
            </div>
          </div>
        )}
      </Form.List>
    </div>
  );
};

const ComputedWatcher: React.FC<{
  field: FieldConfig;
  form: any;
  fieldName: any;
}> = ({ field, form, fieldName }) => {
  const allValues = Form.useWatch([], form);

  useEffect(() => {
    if (field.compute && typeof field.compute === 'function' && form) {
      try {
        const computedVal = field.compute(allValues || {});
        if (computedVal !== undefined) {
          const currentVal = form.getFieldValue(fieldName);
          if (currentVal !== computedVal) {
            form.setFieldValue(fieldName, computedVal);
          }
        }
      } catch (e) {
        console.error(`Error computing value for field ${field.name}:`, e);
      }
    }
  }, [allValues, field, form, fieldName]);

  return null;
};

const FormField: React.FC<FormFieldProps> = ({ field, form, initialValues, name, noLabel, parentName }) => {
  const [options, setOptions] = useState<any[]>([]);
  const [loading, setLoading] = useState(false);

  const fieldName = name !== undefined ? name : field.name;
  const fullPath = parentName
    ? [parentName, ...(Array.isArray(fieldName) ? fieldName : [fieldName])]
    : fieldName;

  useEffect(() => {
    if ((field.type === 'select' || field.type === 'tags') && field.resource) {
      fetchOptions();
    }
  }, [field]);

  // Apply default value for new records (only if no value is currently set)
  useEffect(() => {
    if (field.default !== undefined && form) {
      const currentValue = form.getFieldValue(fullPath);
      if (currentValue === undefined || currentValue === null) {
        form.setFieldValue(fullPath, field.default);
      }
    }
  }, [field.default, form, fullPath]);

  const fetchOptions = async () => {
    try {
      setLoading(true);
      const res = field.resource;
      const service = res?.service;
      let data: any[] = [];
      if (service) {
        if (typeof service.list === 'function') {
          data = await service.list();
        } else if (typeof service.query === 'function') {
          data = await service.query();
        }
      }
      // Handle different response formats
      if (Array.isArray(data)) {
        const mappedOptions = data.map((item: any) => ({ 
          label: item.name || item.label || item.title || item.id, 
          value: item.id || item.value 
        }));
        setOptions(mappedOptions);

        // Auto-map current text/label value (e.g., "ROLE_ADMIN") to its option ID value (e.g., 1)
        const currentValue = form.getFieldValue(fullPath);
        if (currentValue !== undefined && currentValue !== null) {
          const matchingOption = mappedOptions.find(
            (opt) => opt.label === currentValue || String(opt.value) === String(currentValue)
          );
          if (matchingOption && matchingOption.value !== currentValue) {
            form.setFieldValue(fullPath, matchingOption.value);
          }
        }
      }
    } catch (error) {
      message.error(`Failed to fetch options for ${field.label || field.name}`);
      console.error('Error fetching options:', error);
    } finally {
      setLoading(false);
    }
  };

  if (field.type === 'table') {
    return <TableWidget field={field} form={form} />;
  }

  const isReadOnly = Boolean(field.readonly);

  const renderInput = () => {
    switch (field.type) {
      case 'text':
        return <AntInput readOnly={isReadOnly} placeholder={`Enter ${field.label || field.name}`} />;
      
      case 'email':
        return <AntInput readOnly={isReadOnly} type="email" placeholder={`Enter ${field.label || field.name}`} />;
      
      case 'password':
        return <AntInput.Password readOnly={isReadOnly} placeholder={`Enter ${field.label || field.name}`} />;
      
      case 'number':
        return <InputNumber readOnly={isReadOnly} placeholder={`Enter ${field.label || field.name}`} style={{ width: '100%' }} />;
      
      case 'textarea':
        return <AntInput.TextArea readOnly={isReadOnly} rows={4} placeholder={`Enter ${field.label || field.name}`} />;
      
      case 'select':
        return (
          <Select
            disabled={isReadOnly}
            placeholder={`Select ${field.label || field.name}`}
            loading={loading}
            options={options}
            allowClear
            optionFilterProp="label"
          />
        );

      case 'tags':
        return (
          <Select
            disabled={isReadOnly}
            mode="multiple"
            placeholder={`Select ${field.label || field.name}`}
            loading={loading}
            options={options}
            allowClear
            optionFilterProp="label"
          />
        );

      case 'date':
        return <DatePicker disabled={isReadOnly} style={{ width: '100%' }} />;
      
      case 'boolean':
        return (
          <Checkbox disabled={isReadOnly}>
            {noLabel ? undefined : (field.label || field.name)}
          </Checkbox>
        );

      case 'file':
        return (
          <Upload
            accept={field.accept}
            disabled={isReadOnly}
            beforeUpload={(file) => {
              // Validate file size
              if (field.maxSize && file.size > field.maxSize * 1024 * 1024) {
                message.error(`File size must be less than ${field.maxSize}MB`);
                return false;
              }
              return false; // Prevent automatic upload
            }}
            maxCount={1}
            customRequest={({ onSuccess, file }) => {
              // Custom request to prevent actual upload and just store the file
              onSuccess?.(file);
            }}
          >
            <AntInput placeholder={`Select ${field.label || field.name}`} readOnly />
          </Upload>
        );

      default:
        return <AntInput readOnly={isReadOnly} placeholder={`Enter ${field.label || field.name}`} />;
    }
  };

  const buildRules = () => {
    const rules: any[] = [];

    if (field.validations) {
      rules.push({
        validator: async (_rule: any, value: any) => {
          const allValues = form ? form.getFieldsValue() : {};
          const result = validateField(field, value, allValues);
          if (!result.valid && result.error) {
            throw new Error(result.error);
          }
        },
      });
    }

    return rules;
  };

  return (
    <>
      {field.compute && <ComputedWatcher field={field} form={form} fieldName={fullPath} />}
      <Form.Item
        label={noLabel || field.type === 'boolean' ? undefined : (field.label || field.name)}
        name={fieldName}
        rules={buildRules()}
        valuePropName={field.type === 'boolean' ? 'checked' : field.type === 'file' ? 'fileList' : 'value'}
        getValueProps={(val) => {
          if (field.type === 'boolean') {
            return { checked: val };
          }
          if (field.type === 'date' && val) {
            return { value: dayjs.isDayjs(val) ? val : dayjs(val) };
          }
          return { value: val };
        }}
        {...(field.type === 'file' ? { getValueFromEvent: (e: any) => (Array.isArray(e) ? e : e?.fileList) } : {})}
        style={noLabel ? { marginBottom: 0 } : undefined}
      >
        {renderInput()}
      </Form.Item>
    </>
  );
};

export default FormField;
