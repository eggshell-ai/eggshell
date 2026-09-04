'use client';

import React, { useState, useEffect } from 'react';
import { Form, Input as AntInput, Select, Checkbox, Row, Col } from 'antd';
import type { FieldConfig } from '../../types/resource';

/** Name of the hidden form field tracking the "Is Empty" checkbox per field */
export const isEmptyFlagName = (fieldName: string) => `${fieldName}__isEmpty`;

interface FilterFieldProps {
  field: FieldConfig;
}

/**
 * Renders a single filter control for a filterable field.
 *
 * - Text-like fields render a search input plus an explicit "Is Empty" checkbox.
 *   When "Is Empty" is checked, the text value is ignored.
 * - Boolean fields render a dropdown with "Any", "Yes", "No" options.
 *   "Any" (default) means no filtering on this field.
 * - Other field types fall back to a sensible default control.
 */
const FilterField: React.FC<FilterFieldProps> = ({ field }) => {
  const [isEmpty, setIsEmpty] = useState(false);
  const form = Form.useFormInstance();

  const label = field.label || field.name;

  const renderControl = () => {
    switch (field.type) {
      case 'text':
      case 'email':
      case 'textarea':
      case 'password':
      case 'phone':
        return (
          <Row gutter={8} align="middle">
            <Col flex="auto">
              <Form.Item
                name={field.name}
                noStyle
                getValueProps={(val) => ({ value: isEmpty ? undefined : val })}
              >
                <AntInput
                  placeholder={`Search ${label}...`}
                  disabled={isEmpty}
                  allowClear
                />
              </Form.Item>
            </Col>
            <Col flex="none">
              <Form.Item
                name={isEmptyFlagName(field.name)}
                noStyle
                valuePropName="checked"
                initialValue={false}
              >
                <Checkbox
                  onChange={(e) => {
                    setIsEmpty(e.target.checked);
                    if (e.target.checked) {
                      // Clear the text value when switching to "Is Empty" mode
                      form.setFieldValue(field.name, undefined);
                    }
                  }}
                >
                  Is Empty
                </Checkbox>
              </Form.Item>
            </Col>
          </Row>
        );

      case 'boolean':
        return (
          <Form.Item name={field.name} noStyle initialValue="any">
            <Select
              placeholder={`Filter ${label}`}
              allowClear
              options={[
                { label: 'Any', value: 'any' },
                { label: 'Yes', value: 'yes' },
                { label: 'No', value: 'no' },
              ]}
            />
          </Form.Item>
        );

      case 'select':
      case 'foreign':
        return (
          <Form.Item name={field.name} noStyle>
            <Select
              placeholder={`Filter ${label}`}
              allowClear
              options={
                field.resource?.fields
                  ?.filter((f: any) => typeof f !== 'function')
                  .map((f: any) => ({
                    label: f.label || f.name,
                    value: f.name,
                  })) || []
              }
            />
          </Form.Item>
        );

      case 'number':
        return (
          <Form.Item name={field.name} noStyle>
            <AntInput placeholder={`Filter ${label}...`} allowClear />
          </Form.Item>
        );

      default:
        return (
          <Form.Item name={field.name} noStyle>
            <AntInput placeholder={`Filter ${label}...`} allowClear />
          </Form.Item>
        );
    }
  };

  return (
    <Form.Item label={label} style={{ marginBottom: 16 }}>
      {renderControl()}
    </Form.Item>
  );
};

export default FilterField;
