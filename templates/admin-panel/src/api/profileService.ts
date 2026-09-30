import apiService from './apiService';
import { CrudService } from '../types/resource';

export interface ProfileData {
  id?: number | string;
  avatar?: string | null;
  user?: any;
  [key: string]: any;
}

const profileService: CrudService<ProfileData> & {
  getMyProfile: () => Promise<ProfileData>;
  updateMyProfile: (data: any) => Promise<ProfileData>;
} = {
  endpoint: '/profile',

  list: async (config = {}) => {
    return apiService.get<ProfileData[]>('/profile', config);
  },

  query: async (config = {}) => {
    return apiService.get<ProfileData[]>('/profile', config);
  },

  get: async (id: string | number, config = {}) => {
    return apiService.get<ProfileData>(`/profile/${id}`, config);
  },

  create: async (data: Partial<ProfileData>, config = {}) => {
    return apiService.post<ProfileData>('/profile', data, config);
  },

  update: async (id: string | number, data: Partial<ProfileData>, config = {}) => {
    const isFormData = typeof FormData !== 'undefined' && data instanceof FormData;
    const requestConfig = isFormData
      ? { ...config, headers: { ...config?.headers, 'Content-Type': 'multipart/form-data' } }
      : config;
    return apiService.post<ProfileData>(`/profile/${id}`, data, requestConfig);
  },

  delete: async (id: string | number, config = {}) => {
    return apiService.delete(`/profile/${id}`, config);
  },

  getMyProfile: async () => {
    return apiService.get<ProfileData>('/profile/me');
  },

  updateMyProfile: async (data: any) => {
    const isFormData = typeof FormData !== 'undefined' && data instanceof FormData;
    const config = isFormData ? { headers: { 'Content-Type': 'multipart/form-data' } } : {};
    return apiService.post<ProfileData>('/profile/me', data, config);
  }
};

export default profileService;
