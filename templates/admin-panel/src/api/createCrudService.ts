import apiService from './apiService';
import { CrudService } from '../types/resource';

/**
 * Factory function to create a CRUD service from an endpoint
 * Automatically generates REST-based operations for a resource
 */
export function createCrudService<T = any>(endpoint: string): CrudService<T> {
  return {
    list: async (config = {}) => {
      return apiService.get<T[]>(endpoint, config);
    },

    query: async (config = {}) => {
      return apiService.get<T[]>(endpoint, config);
    },

    get: async (id: string | number, config = {}) => {
      return apiService.get<T>(`${endpoint}/${id}`, config);
    },

    create: async (data: Partial<T>, config = {}) => {
      return apiService.post<T>(endpoint, data, config);
    },

    update: async (id: string | number, data: Partial<T>, config = {}) => {
      return apiService.put<T>(`${endpoint}/${id}`, data, config);
    },

    delete: async (id: string | number, config = {}) => {
      return apiService.delete(`${endpoint}/${id}`, config);
    },
  };
}

export default createCrudService;
