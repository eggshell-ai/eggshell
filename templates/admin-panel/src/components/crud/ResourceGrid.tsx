'use client';

import React, { useState, useEffect } from 'react';
import { Table, Button, Input, Space, Typography, Tag, Badge, Image, Modal, message } from 'antd';
import type { ColumnsType, TableProps } from 'antd/es/table';
import { PlusOutlined, FilterOutlined, CloseCircleFilled } from '@ant-design/icons';
import type { FieldConfig, ResourceAction } from '../../types/resource';
import apiService from '../../api/apiService';
import authService from '../../services/authService';

interface ResourceGridProps {
  source: (() => Promise<any[]>) | any[];
  fields?: FieldConfig[];
  columns?: ColumnsType<any>;
  rowKey?: string;
  onAdd?: () => void;
  onBulkDelete?: (selectedRowKeys: React.Key[]) => Promise<void>;
  searchPlaceholder?: string;
  onSearch?: (value: string) => void;
  filterPanel?: React.ReactNode;
  filters?: Record<string, any> | null;
  onClearFilters?: () => void;
  /** Custom row actions (e.g. activate/deactivate) rendered in the actions column */
  actions?: ResourceAction[];
  /** Base endpoint used to execute custom actions (e.g. '/users') */
  endpoint?: string;
}

export default function ResourceGrid({
  source,
  fields,
  columns,
  rowKey = 'id',
  onAdd,
  onBulkDelete,
  searchPlaceholder = 'Search...',
  onSearch,
  filterPanel,
  filters,
  onClearFilters,
  actions,
  endpoint,
}: ResourceGridProps) {
  const [data, setData] = useState<any[]>([]);
  const [loading, setLoading] = useState(false);
  const [selectedRowKeys, setSelectedRowKeys] = useState<React.Key[]>([]);
  const [deleteLoading, setDeleteLoading] = useState(false);
  const [filterPanelOpen, setFilterPanelOpen] = useState(false);
  const [allowedActions, setAllowedActions] = useState<Record<number, boolean>>({});
  const [actionLoadingKeys, setActionLoadingKeys] = useState<string[]>([]);

  const activeFilterCount = filters ? Object.keys(filters).length : 0;
  const hasFilterPanel = Boolean(filterPanel);

  useEffect(() => {
    fetchData();
  }, [source]);

  // Check permissions for custom row actions (superusers always pass)
  useEffect(() => {
    const checkActionPermissions = async () => {
      if (!actions || actions.length === 0) {
        setAllowedActions({});
        return;
      }

      const user: any = authService.getUser();
      const isSuperuser =
        user?.role === 'ADMIN' || user?.superuser === true || user?.isSuperuser === true;

      const results: Record<number, boolean> = {};
      await Promise.all(
        actions.map(async (action, index) => {
          results[index] =
            isSuperuser || (await authService.hasPermission(action.permission));
        })
      );
      setAllowedActions(results);
    };

    checkActionPermissions();
  }, [actions]);

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

  /**
   * Evaluate an action expression (e.g. "data.status == 'Active' ? 'Deactivate' : 'Activate'")
   * against a record, with the record available as `data`.
   */
  const evaluateActionExpression = (expression: string | undefined, record: any): any => {
    if (!expression) return undefined;
    try {
      const data = record;
      // eslint-disable-next-line no-eval
      return eval(expression);
    } catch (error) {
      console.error('Error evaluating action expression:', error);
      return undefined;
    }
  };

  const executeAction = async (
    action: ResourceAction,
    actionPath: string,
    label: string,
    record: any
  ) => {
    if (!endpoint) {
      message.error('This resource does not define an endpoint for custom actions');
      console.error('[ResourceGrid] Cannot execute action: resource has no endpoint');
      return;
    }

    const loadingKey = `${record[rowKey]}:${actionPath}`;
    const run = async () => {
      try {
        setActionLoadingKeys((keys) => [...keys, loadingKey]);
        await apiService.post(`${endpoint}/${record[rowKey]}${actionPath}`, {});
        message.success(`${label} completed successfully`);
        await fetchData();
      } catch (error) {
        message.error(`Failed to ${label.toLowerCase()} record`);
        console.error('Error executing action:', error);
      } finally {
        setActionLoadingKeys((keys) => keys.filter((key) => key !== loadingKey));
      }
    };

    if (action.confirm) {
      Modal.confirm({
        title: 'Confirm Action',
        content: `Are you sure you want to ${label.toLowerCase()} this record?`,
        okText: label,
        cancelText: 'Cancel',
        onOk: run,
      });
    } else {
      await run();
    }
  };

  /** Render custom action buttons for a row (only permitted actions with truthy labels) */
  const renderCustomActions = (record: any): React.ReactNode[] => {
    if (!actions || actions.length === 0) return [];

    const buttons: React.ReactNode[] = [];
    actions.forEach((action, index) => {
      if (!allowedActions[index]) return;

      const label = evaluateActionExpression(action.labelExpression, record);
      if (!label) return; // falsy label hides the button for this row

      const actionPath = evaluateActionExpression(action.actionExpression, record);
      if (!actionPath) return;

      const loadingKey = `${record[rowKey]}:${actionPath}`;
      buttons.push(
        <Button
          key={`action-${index}`}
          type="link"
          size="small"
          loading={actionLoadingKeys.includes(loadingKey)}
          onClick={() => executeAction(action, actionPath, label, record)}
        >
          {label}
        </Button>
      );
    });
    return buttons;
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
            text={value ? (field.trueLabel || 'Yes') : (field.falseLabel || 'No')} 
          />
        );
      
      case 'email':
        return value ? <a href={`mailto:${value}`}>{value}</a> : '-';
      
      case 'date':
        return value ? new Date(value).toLocaleDateString() : '-';
      
      case 'time':
        return value ? String(value) : '-';
      
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

    let allColumns: ColumnsType<any> = columns ? [...fieldColumns, ...columns] : fieldColumns;

    // Merge custom action buttons into an existing Actions column, or append one
    if (actions && actions.length > 0) {
      const actionsColIndex = allColumns.findIndex((col: any) => col && col.key === 'actions');
      if (actionsColIndex >= 0) {
        allColumns = allColumns.map((col: any, index: number) => {
          if (index !== actionsColIndex) return col;
          const originalRender = col.render;
          return {
            ...col,
            render: (text: any, record: any, rowIndex: number) => (
              <Space>
                {originalRender ? originalRender(text, record, rowIndex) : null}
                {renderCustomActions(record)}
              </Space>
            ),
          };
        });
      } else {
        allColumns = [
          ...allColumns,
          {
            title: 'Actions',
            key: 'actions',
            render: (_: any, record: any) => <Space>{renderCustomActions(record)}</Space>,
          },
        ];
      }
    }

    return allColumns;
  };

  const tableColumns = transformFieldsToColumns();

  return (
    <div>
      {filterPanelOpen && filterPanel}
      <div style={{ marginBottom: 16, display: 'flex', alignItems: 'center' }}>
        <Typography.Text type="secondary" style={{ marginRight: 16 }}>
          {loading ? 'Loading...' : `${data.length} ${data.length === 1 ? 'record' : 'records'}`}
        </Typography.Text>
        {selectedRowKeys.length > 0 && (
          <Space style={{ marginRight: 16 }}>
            <Typography.Text>{selectedRowKeys.length} selected</Typography.Text>
            <Button type="primary" danger onClick={handleBulkDelete} loading={deleteLoading}>
              Delete
            </Button>
          </Space>
        )}
        <Space style={{ marginLeft: 'auto' }}>
          {hasFilterPanel && (
            <Badge count={activeFilterCount} size="small" offset={[-2, 2]}>
              <Button
                icon={<FilterOutlined />}
                onClick={() => setFilterPanelOpen((open) => !open)}
              >
                Filter
              </Button>
            </Badge>
          )}
          <Input.Search
            placeholder={searchPlaceholder}
            style={{ width: 250 }}
            onSearch={onSearch ? (value) => onSearch(value) : undefined}
            allowClear
          />
          {activeFilterCount > 0 && onClearFilters && (
            <Button
              type="text"
              icon={<CloseCircleFilled style={{ color: '#faad14' }} />}
              onClick={onClearFilters}
            >
              Clear Filters
            </Button>
          )}
          {onAdd && (
            <Button type="primary" icon={<PlusOutlined />} onClick={onAdd}>
              Add
            </Button>
          )}
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
