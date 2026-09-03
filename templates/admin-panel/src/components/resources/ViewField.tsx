'use client';

import React from 'react';
import { Image, Typography } from 'antd';
import dayjs from 'dayjs';
import type { FieldConfig } from '../../types/resource';

interface ViewFieldProps {
  field: FieldConfig;
  record: any;
  noLabel?: boolean;
}

const formatColumnLabel = (name: string): string => {
  if (!name) return '';
  return name
    .replace(/_/g, ' ')
    .replace(/([a-z])([A-Z])/g, '$1 $2')
    .replace(/\b\w/g, (char) => char.toUpperCase());
};

/**
 * Read-only display of a single record field value.
 * Mirrors FormField's type handling but renders plain data instead of inputs.
 */
const ViewField: React.FC<ViewFieldProps> = ({ field, record, noLabel }) => {
  const label = field.label || formatColumnLabel(field.name);
  const value = record?.[field.name];

  // Relation fields display their resolved title (e.g. customer_title)
  const titleKey = `${field.name}_title`;
  const altTitleKey = field.name.endsWith('_id') ? `${field.name.slice(0, -3)}_title` : undefined;
  const relationTitle = record?.[titleKey] ?? (altTitleKey ? record?.[altTitleKey] : undefined);

  // Child table fields display their summary (e.g. items_summary)
  const summaryKey = `${field.name}_summary`;
  const tableSummary = record?.[summaryKey];

  const renderValue = (): React.ReactNode => {
    switch (field.type) {
      case 'boolean':
        return value ? 'Yes' : 'No';

      case 'date':
        if (!value) return '-';
        try {
          return dayjs(value).format('YYYY-MM-DD HH:mm:ss');
        } catch {
          return String(value);
        }

      case 'select':
      case 'foreign':
        if (relationTitle !== undefined && relationTitle !== null) {
          return String(relationTitle);
        }
        return value !== undefined && value !== null ? String(value) : '-';

      case 'tags':
        if (Array.isArray(value)) {
          return value.length > 0 ? value.map((v) => String(v)).join(', ') : '-';
        }
        if (relationTitle) return String(relationTitle);
        return value !== undefined && value !== null ? String(value) : '-';

      case 'table':
        if (tableSummary !== undefined && tableSummary !== null) {
          return String(tableSummary);
        }
        if (Array.isArray(value)) {
          if (value.length === 0) return '-';
          return value
            .map((item: any) => {
              if (item && typeof item === 'object') {
                return item.title || item.name || item.label || `#${item.id ?? ''}`;
              }
              return String(item);
            })
            .join(', ');
        }
        return value !== undefined && value !== null ? String(value) : '-';

      case 'file':
        if (!value) return '-';
        if (typeof value === 'string') {
          const isImage = /\.(png|jpe?g|gif|webp|svg)$/i.test(value);
          if (isImage) {
            return <Image src={value} width={120} />;
          }
          return <a href={value} target="_blank" rel="noopener noreferrer">{value}</a>;
        }
        return String(value);

      case 'password':
        return value ? '••••••••' : '-';

      default:
        if (value === undefined || value === null || value === '') {
          return '-';
        }
        if (typeof value === 'object') {
          return JSON.stringify(value);
        }
        return String(value);
    }
  };

  if (noLabel) {
    return <div style={{ color: '#262626' }}>{renderValue()}</div>;
  }

  return (
    <div style={{ marginBottom: 16 }}>
      <Typography.Text type="secondary" style={{ fontSize: 12, display: 'block' }}>
        {label}
      </Typography.Text>
      <Typography.Text style={{ fontSize: 14 }}>{renderValue()}</Typography.Text>
    </div>
  );
};

export default ViewField;
