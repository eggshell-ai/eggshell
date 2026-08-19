import apiService from './apiService';
import { CrudService } from '@/types/resource';
import { Role } from '@/types/role';

const roleService: CrudService<Role> = {
  list: async (config = {}) => {
    return apiService.get<Role[]>('/roles', config);
  },

  query: async (config = {}) => {
    return apiService.get<Role[]>('/roles', config);
  },

  get: async (id: string | number, config = {}) => {
    return apiService.get<Role>(`/roles/${id}`, config);
  },

  create: async (data: Partial<Role>, config = {}) => {
    return apiService.post<Role>('/roles', data, config);
  },

  update: async (id: string | number, data: Partial<Role>, config = {}) => {
    return apiService.put<Role>(`/roles/${id}`, data, config);
  },

  delete: async (id: string | number, config = {}) => {
    return apiService.delete(`/roles/${id}`, config);
  },
};

export default roleService;
