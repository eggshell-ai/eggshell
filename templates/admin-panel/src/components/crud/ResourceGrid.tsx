'use client';

import React, { useState, useEffect } from 'react';
import { Table, Button, Input, Space, Typography, Tag, Badge, Image } from 'antd';
import type { ColumnsType, TableProps } from 'antd/es/table';
import { PlusOutlined } from '@ant-design/icons';
import type { FieldConfig } from '../../types/resource';
import apiService from '../../api/apiService';

interface ResourceGridProps {
  source: (() => Promise<any[]>) | any[];
  fields?: FieldConfig[];
  columns?: ColumnsType<any>;
  rowKey?: string;
  onAdd?: () => void;
  onBulkDelete?: (selectedRowKeys: React.Key[]) => Promise<void>;
  searchPlaceholder?: string;
}

export default function ResourceGrid({
  source,
  fields,
  columns,
  rowKey = 'id',
  onAdd,
  onBulkDelete,
  searchPlaceholder = 'Search...'
}: ResourceGridProps) {
  const [data, setData] = useState<any[]>([]);
  const [loading, setLoading] = useState(false);
  const [selectedRowKeys, setSelectedRowKeys] = useState<React.Key[]>([]);
  const [deleteLoading, setDeleteLoading] = useState(false);

  useEffect(() => {
    fetchData();
  }, [source]);

  const fetchData = async () => {
    try {
      setLoading(true);
      if (typeof source === 'function') {
        const result = await source();
        setData(result);
      } else {
        setData(source);
      }
    } catch (error) {
      console.error('Error fetching data:', error);
    } finally {
      setLoading(false);
    }
  };

  const onSelectChange = (newSelectedRowKeys: React.Key[]) => {
    setSelectedRowKeys(newSelectedRowKeys);
  };

  const rowSelection: TableProps<any>['rowSelection'] = {
    selectedRowKeys,
    onChange: onSelectChange,
  };

  const handleBulkDelete = async () => {
    if (onBulkDelete) {
      try {
        setDeleteLoading(true);
        await onBulkDelete(selectedRowKeys);
        setSelectedRowKeys([]);
        await fetchData();
      } catch (error) {
        console.error('Error during bulk delete:', error);
      } finally {
        setDeleteLoading(false);
      }
    }
  };

  const renderCell = (field: FieldConfig, record: any) => {
    const value = record[field.name];
    
    // Check for relation field titles (e.g. customer_title, product_title, invoice_id_title / invoice_title)
    const titleKey = `${field.name}_title`;
    const altTitleKey = field.name.endsWith('_id') ? `${field.name.slice(0, -3)}_title` : undefined;
    const relationTitle = record[titleKey] ?? (altTitleKey ? record[altTitleKey] : undefined);

    // Check for child table summary (e.g. items_summary)
    const summaryKey = `${field.name}_summary`;
    const tableSummary = record[summaryKey];

    switch (field.type) {
      case 'select':
      case 'foreign':
        if (relationTitle !== undefined && relationTitle !== null) {
          return relationTitle;
        }
        return value !== undefined && value !== null ? String(value) : '-';

      case 'table':
        if (tableSummary !== undefined && tableSummary !== null) {
          return tableSummary;
        }
        if (Array.isArray(value)) {
          if (value.length === 0) return '-';
          const summaries = value.map((item: any) => {
            if (typeof item === 'object' && item !== null) {
              if (item.title) return item.title;
              if (item.name) return item.name;
              const productTitle = item.product_title || item.product;
              if (productTitle && item.qty) {
                return `${item.qty}x ${productTitle}${item.rate !== undefined ? ` (@${item.rate.toLocaleString()})` : ''}`;
              }
              return JSON.stringify(item);
            }
            return String(item);
          });
          return summaries.join(', ');
        }
        return value ? String(value) : '-';

      case 'tags':
        if (Array.isArray(value)) {
          return (
            <Space size="small" wrap>
              {value.map((tag: string, index: number) => (
                <Tag key={index} color="blue">
                  {tag}
                </Tag>
              ))}
            </Space>
          );
        }
        return value ? <Tag color="blue">{value}</Tag> : '-';
      
      case 'boolean':
        return (
          <Badge 
            status={value ? 'success' : 'default'} 
            text={value ? 'Yes' : 'No'} 
          />
        );
      
      case 'email':
        return value ? <a href={`mailto:${value}`}>{value}</a> : '-';
      
      case 'date':
        return value ? new Date(value).toLocaleDateString() : '-';
      
      case 'number':
        return value !== undefined && value !== null ? value : '-';
      
      case 'textarea':
        return value ? (
          <Typography.Text ellipsis={{ tooltip: value }} style={{ maxWidth: 200 }}>
            {value}
          </Typography.Text>
        ) : '-';

      case 'file':
        if (value) {
          const mediaUrl = apiService.getMediaUrl(value);
          return (
            <Image
              src={mediaUrl}
              alt={field.label || field.name}
              width={60}
              height={60}
              style={{ objectFit: 'cover', borderRadius: 4 }}
            />
          );
        }
        return '-';

      case 'text':
      default:
        if (relationTitle !== undefined && relationTitle !== null) {
          return relationTitle;
        }
        if (tableSummary !== undefined && tableSummary !== null) {
          return tableSummary;
        }
        if (typeof value === 'object' && value !== null) {
          if (Array.isArray(value)) {
            return value.map(i => (typeof i === 'object' ? JSON.stringify(i) : String(i))).join(', ');
          }
          return JSON.stringify(value);
        }
        return value !== undefined && value !== null && value !== '' ? value : '-';
    }
  };

  const transformFieldsToColumns = (): ColumnsType<any> => {
    const fieldColumns = !fields ? [] : fields
      .filter(field => field.table !== false)
      .map(field => ({
        title: field.label || field.name,
        dataIndex: field.name,
        key: field.name,
        render: (_: any, record: any) => renderCell(field, record),
      }));
    
    if (columns) {
      return [...fieldColumns, ...columns];
    }
    
    return fieldColumns;
  };

  const tableColumns = transformFieldsToColumns();

  return (
    <div>
      <div style={{ marginBottom: 16, display: 'flex', alignItems: 'center' }}>
        {selectedRowKeys.length > 0 && (
          <Space style={{ marginRight: 16 }}>
            <Typography.Text>{selectedRowKeys.length} selected</Typography.Text>
            <Button type="primary" danger onClick={handleBulkDelete} loading={deleteLoading}>
              Delete
            </Button>
          </Space>
        )}
        <Space style={{ marginLeft: 'auto' }}>
          {onAdd && (
            <Button type="primary" icon={<PlusOutlined />} onClick={onAdd}>
              Add
            </Button>
          )}
          <Input.Search placeholder={searchPlaceholder} style={{ width: 250 }} />
        </Space>
      </div>

      <Table
        rowSelection={rowSelection}
        columns={tableColumns}
        dataSource={data}
        rowKey={rowKey}
        loading={loading}
      />
    </div>
  );
}
