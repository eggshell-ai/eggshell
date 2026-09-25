'use client';

import React, { useState, useEffect } from 'react';
import { Table, Card, Button, Input, Space, DatePicker, Select, Tag } from 'antd';
import { SearchOutlined, ReloadOutlined } from '@ant-design/icons';
import type { ColumnsType } from 'antd/es/table';
import apiService from '../../api/apiService';

interface DataTableFilter {
  name: string;
  type: 'select' | 'dateRange' | 'text';
  label?: string;
  placeholder?: string;
  options?: Record<string, string> | { label: string; value: any }[];
}

interface DataTableProps {
  endpoint: string;
  columns: ColumnsType<any>;
  filters?: DataTableFilter[];
  title?: string;
  rowKey?: string;
  params?: Record<string, any>;
  pageSize?: number;
}

export default function DataTable({
  endpoint,
  columns,
  filters = [],
  title,
  rowKey = 'id',
  params = {},
  pageSize = 10,
}: DataTableProps) {
  const [data, setData] = useState<any[]>([]);
  const [loading, setLoading] = useState(false);
  const [filterValues, setFilterValues] = useState<Record<string, any>>({});
  const [searchTerm, setSearchTerm] = useState('');
  const [debouncedSearchTerm, setDebouncedSearchTerm] = useState('');

  // Debounce search term changes to prevent spamming requests on typing
  useEffect(() => {
    const timer = setTimeout(() => {
      setDebouncedSearchTerm(searchTerm);
    }, 350);
    return () => clearTimeout(timer);
  }, [searchTerm]);

  const serializedParams = JSON.stringify(params);
  const serializedFilters = JSON.stringify(filterValues);

  useEffect(() => {
    let isMounted = true;

    const fetchData = async () => {
      try {
        setLoading(true);
        const queryParams = new URLSearchParams();

        // Attach params prop
        Object.entries(params).forEach(([k, v]) => {
          if (v !== undefined && v !== null && v !== '') {
            queryParams.append(k, String(v));
          }
        });

        // Attach filter values
        Object.entries(filterValues).forEach(([k, v]) => {
          if (v !== undefined && v !== null && v !== '') {
            if (Array.isArray(v)) {
              if (v[0] !== undefined && v[0] !== '') {
                queryParams.append(`filters[${k}][0]`, String(v[0]));
              }
              if (v[1] !== undefined && v[1] !== '') {
                queryParams.append(`filters[${k}][1]`, String(v[1]));
              }
            } else {
              queryParams.append(`filters[${k}]`, String(v));
            }
          }
        });

        if (debouncedSearchTerm) {
          queryParams.append('search', debouncedSearchTerm);
        }

        // Clean leading /api or api/ from endpoint so apiService baseURL does not create /api/api/
        const normalizedEndpoint = endpoint.replace(/^\/?api(\/|$)/, '/');
        const queryString = queryParams.toString();
        const url = `${normalizedEndpoint}${queryString ? (normalizedEndpoint.includes('?') ? '&' : '?') + queryString : ''}`;
        const res = await apiService.get(url);

        if (isMounted) {
          const items = Array.isArray(res) ? res : (res?.data || res?.items || []);
          setData(items);
        }
      } catch (err) {
        if (isMounted) {
          console.error('Error fetching DataTable data:', err);
        }
      } finally {
        if (isMounted) {
          setLoading(false);
        }
      }
    };

    fetchData();

    return () => {
      isMounted = false;
    };
  }, [endpoint, serializedFilters, debouncedSearchTerm, serializedParams]);

  const handleReload = () => {
    // Manually trigger a re-fetch by updating state or invoking reload
    const queryParams = new URLSearchParams();
    Object.entries(params).forEach(([k, v]) => {
      if (v !== undefined && v !== null && v !== '') queryParams.append(k, String(v));
    });
    Object.entries(filterValues).forEach(([k, v]) => {
      if (v !== undefined && v !== null && v !== '') {
        if (Array.isArray(v)) {
          if (v[0]) queryParams.append(`filters[${k}][0]`, String(v[0]));
          if (v[1]) queryParams.append(`filters[${k}][1]`, String(v[1]));
        } else {
          queryParams.append(`filters[${k}]`, String(v));
        }
      }
    });
    if (debouncedSearchTerm) queryParams.append('search', debouncedSearchTerm);
    const normalizedEndpoint = endpoint.replace(/^\/?api(\/|$)/, '/');
    const queryString = queryParams.toString();
    const url = `${normalizedEndpoint}${queryString ? (normalizedEndpoint.includes('?') ? '&' : '?') + queryString : ''}`;

    setLoading(true);
    apiService.get(url)
      .then((res) => {
        const items = Array.isArray(res) ? res : (res?.data || res?.items || []);
        setData(items);
      })
      .catch((err) => console.error('Error fetching DataTable data:', err))
      .finally(() => setLoading(false));
  };

  const handleFilterChange = (filterName: string, value: any) => {
    setFilterValues((prev) => ({
      ...prev,
      [filterName]: value,
    }));
  };

  const handleReset = () => {
    setFilterValues({});
    setSearchTerm('');
  };

  return (
    <Card
      title={title}
      extra={
        <Button icon={<ReloadOutlined />} onClick={handleReload}>
          Refresh
        </Button>
      }
      style={{ marginBottom: 24 }}
    >
      {(filters.length > 0 || true) && (
        <Space wrap style={{ marginBottom: 16 }}>
          <Input
            placeholder="Search records..."
            prefix={<SearchOutlined />}
            value={searchTerm}
            onChange={(e) => setSearchTerm(e.target.value)}
            style={{ width: 200 }}
            allowClear
          />

          {filters.map((f) => {
            if (f.type === 'select') {
              const opts = Array.isArray(f.options)
                ? f.options
                : f.options
                ? Object.entries(f.options).map(([val, label]) => ({ label: String(label), value: val }))
                : [];
              return (
                <Select
                  key={f.name}
                  placeholder={f.placeholder || f.label || `Select ${f.name}`}
                  value={filterValues[f.name]}
                  onChange={(val) => handleFilterChange(f.name, val)}
                  options={opts}
                  allowClear
                  style={{ minWidth: 160 }}
                />
              );
            }

            if (f.type === 'dateRange') {
              return (
                <DatePicker.RangePicker
                  key={f.name}
                  value={filterValues[f.name]}
                  onChange={(dates, dateStrings) => handleFilterChange(f.name, dateStrings)}
                />
              );
            }

            return (
              <Input
                key={f.name}
                placeholder={f.placeholder || f.label || f.name}
                value={filterValues[f.name]}
                onChange={(e) => handleFilterChange(f.name, e.target.value)}
                style={{ width: 160 }}
                allowClear
              />
            );
          })}

          <Button onClick={handleReset}>Clear</Button>
        </Space>
      )}

      <Table
        columns={columns}
        dataSource={data}
        rowKey={rowKey}
        loading={loading}
        pagination={{ pageSize, showSizeChanger: true }}
      />
    </Card>
  );
}
