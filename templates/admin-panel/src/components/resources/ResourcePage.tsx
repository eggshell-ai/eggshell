'use client';

import React, { useState, useEffect } from 'react';
import { Modal, Form, message, Button, Space } from 'antd';
import { EditOutlined, DeleteOutlined } from '@ant-design/icons';
import type { Resource } from '../../types/resource';
import ResourceGrid from '../crud/ResourceGrid';
import ResourceForm, { processResourceFormValues, ResourceFilterForm } from './ResourceForm';
import apiService from '../../api/apiService';
import authService from '../../services/authService';

interface ResourcePageProps {
  resource: Resource;
}

const ResourcePage: React.FC<ResourcePageProps> = ({ resource }) => {
  const [deleteModalOpen, setDeleteModalOpen] = useState(false);
  const [editModalOpen, setEditModalOpen] = useState(false);
  const [addModalOpen, setAddModalOpen] = useState(false);
  const [selectedRecord, setSelectedRecord] = useState<any>(null);
  const [deleteLoading, setDeleteLoading] = useState(false);
  const [filters, setFilters] = useState<Record<string, any> | null>(null);
  const [form] = Form.useForm();
  const [addForm] = Form.useForm();
  const [permissions, setPermissions] = useState({
    canView: false,
    canCreate: false,
    canEdit: false,
    canDelete: false,
  });
  const [permissionsChecked, setPermissionsChecked] = useState(false);

  useEffect(() => {
    const checkPermissions = async () => {
      const viewPerm = resource.permissions?.view;
      const createPerm = resource.permissions?.create;
      const editPerm = resource.permissions?.edit;
      const deletePerm = resource.permissions?.delete;

      const [canView, canCreate, canEdit, canDelete] = await Promise.all([
        authService.hasPermission(viewPerm),
        authService.hasPermission(createPerm),
        authService.hasPermission(editPerm),
        authService.hasPermission(deletePerm),
      ]);

      setPermissions({ canView, canCreate, canEdit, canDelete });
      setPermissionsChecked(true);
    };

    checkPermissions();
  }, [resource.permissions]);

  if (!permissionsChecked) {
    return null;
  }

  if (!permissions.canView) {
    return null;
  }

  const handleDelete = (record: any) => {
    setSelectedRecord(record);
    setDeleteModalOpen(true);
  };

  const confirmDelete = async () => {
    try {
      setDeleteLoading(true);
      await resource.service.delete(selectedRecord.id);
      setDeleteModalOpen(false);
      setSelectedRecord(null);
      message.success('Record deleted successfully');
    } catch (error) {
      message.error('Failed to delete record');
      console.error('Error deleting record:', error);
    } finally {
      setDeleteLoading(false);
    }
  };

  const handleEdit = (record: any) => {
    setSelectedRecord(record);
    form.setFieldsValue(record);
    setEditModalOpen(true);
  };

  const handleEditSubmit = async () => {
    try {
      const values = await form.validateFields();
      const payload = processResourceFormValues(values, resource.fields);
      await resource.service.update(selectedRecord.id, payload);
      message.success('Record updated successfully');
      setEditModalOpen(false);
      setSelectedRecord(null);
      form.resetFields();
    } catch (error: any) {
      if (error.errorFields) {
        return;
      }
      message.error('Failed to update record');
      console.error('Error updating record:', error);
    }
  };

  const handleAdd = () => {
    setAddModalOpen(true);
  };

  const handleAddSubmit = async () => {
    try {
      const values = await addForm.validateFields();
      const payload = processResourceFormValues(values, resource.fields);
      await resource.service.create(payload);
      message.success('Record created successfully');
      setAddModalOpen(false);
      addForm.resetFields();
    } catch (error: any) {
      if (error.errorFields) {
        return;
      }
      message.error('Failed to create record');
      console.error('Error creating record:', error);
    }
  };

  const handleBulkDelete = async (selectedRowKeys: any[]) => {
    try {
      await Promise.all(selectedRowKeys.map((id) => resource.service.delete(id)));
      message.success(`${selectedRowKeys.length} record(s) deleted successfully`);
    } catch (error) {
      message.error('Failed to delete records');
      console.error('Error deleting records:', error);
      throw error;
    }
  };

  const actionsColumn: any = {
    title: 'Actions',
    key: 'actions',
    render: (_: any, record: any) => (
      <Space>
        {permissions.canEdit && (
          <Button type="link" size="small" icon={<EditOutlined />} onClick={() => handleEdit(record)} />
        )}
        {permissions.canDelete && (
          <Button type="link" size="small" danger icon={<DeleteOutlined />} onClick={() => handleDelete(record)} />
        )}
      </Space>
    ),
  };

  const hasFilterableFields = resource.fields.some(
    (field) => field.filterable === true
  );

  const handleApplyFilters = (newFilters: Record<string, any> | null) => {
    setFilters(newFilters);
  };

  const handleClearFilters = () => {
    setFilters(null);
  };

  return (
    <div>
      <h1>{resource.name}</h1>
      <ResourceGrid
        source={() => resource.service.list(filters ? { params: { filters } } : undefined)}
        fields={resource.fields}
        columns={permissions.canEdit || permissions.canDelete ? [actionsColumn] : undefined}
        onAdd={permissions.canCreate ? handleAdd : undefined}
        onBulkDelete={permissions.canDelete ? handleBulkDelete : undefined}
        searchPlaceholder={`Search ${resource.name}...`}
        filterPanel={
          hasFilterableFields ? (
            <ResourceFilterForm
              resource={resource}
              filters={filters}
              onApply={handleApplyFilters}
            />
          ) : undefined
        }
        filters={filters}
        onClearFilters={hasFilterableFields ? handleClearFilters : undefined}
      />

      <Modal
        title="Delete Record"
        open={deleteModalOpen}
        onOk={confirmDelete}
        onCancel={() => {
          setDeleteModalOpen(false);
          setSelectedRecord(null);
        }}
        okText="Delete"
        cancelText="Cancel"
        okButtonProps={{ danger: true, loading: deleteLoading }}
        confirmLoading={deleteLoading}
      >
        <p>Are you sure you want to delete this record?</p>
      </Modal>

      <Modal
        title="Edit Record"
        open={editModalOpen}
        onOk={handleEditSubmit}
        onCancel={() => {
          setEditModalOpen(false);
          setSelectedRecord(null);
          form.resetFields();
        }}
        okText="Save"
        cancelText="Cancel"
        width="65%"
      >
        <ResourceForm resource={resource} form={form} />
      </Modal>

      <Modal
        title="Add Record"
        open={addModalOpen}
        onOk={handleAddSubmit}
        onCancel={() => {
          setAddModalOpen(false);
          addForm.resetFields();
        }}
        okText="Create"
        cancelText="Cancel"
        width="65%"
      >
        <ResourceForm resource={resource} form={addForm} />
      </Modal>
    </div>
  );
};

export default ResourcePage;
