import apiService from './apiService';
import { CrudService } from '../types/resource';
import { User } from '../types/user';

const userService: CrudService<User> = {
  list: async (config = {}) => {
    return apiService.get<User[]>('/users', config);
  },

  query: async (config = {}) => {
    return apiService.get<User[]>('/users', config);
  },

  get: async (id: string | number, config = {}) => {
    return apiService.get<User>(`/users/${id}`, config);
  },

  create: async (data: Partial<User>, config = {}) => {
    return apiService.post<User>('/users', data, config);
  },

  update: async (id: string | number, data: Partial<User>, config = {}) => {
    return apiService.put<User>(`/users/${id}`, data, config);
  },

  delete: async (id: string | number, config = {}) => {
    return apiService.delete(`/users/${id}`, config);
  },
};

export default userService;
