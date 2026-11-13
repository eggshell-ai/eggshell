import axios, { AxiosRequestConfig, AxiosResponse } from 'axios';
import authService from '../services/authService';

const API_URL = process.env.NEXT_PUBLIC_API_URL || 'http://localhost:8000/api';
const BASE_URL = API_URL.replace(/\/api\/?$/, '');

const api = axios.create({
  baseURL: API_URL,
  headers: {
    'Content-Type': 'application/json',
  },
});

// Request interceptor to add auth token if available
api.interceptors.request.use(
  (config) => {
    const token = authService.getToken();
    if (token) {
      config.headers.Authorization = `Bearer ${token}`;
    }
    if (config.data instanceof FormData) {
      delete config.headers['Content-Type'];
    }
    return config;
  },
  (error) => {
    return Promise.reject(error);
  }
);

// Response interceptor to handle errors
api.interceptors.response.use(
  (response) => response,
  (error) => {
    if (error.response?.status === 401) {
      // Handle unauthorized access - delegate to authService
      authService.logout();
      window.location.href = '/login';
    }
    return Promise.reject(error);
  }
);

const apiService = {
  get: async <T = any>(url: string, config?: AxiosRequestConfig): Promise<T> => {
    const response: AxiosResponse<T> = await api.get(url, config);
    return response.data;
  },

  post: async <T = any>(url: string, data: any, config?: AxiosRequestConfig): Promise<T> => {
    const response: AxiosResponse<T> = await api.post(url, data, config);
    return response.data;
  },

  put: async <T = any>(url: string, data: any, config?: AxiosRequestConfig): Promise<T> => {
    const response: AxiosResponse<T> = await api.put(url, data, config);
    return response.data;
  },

  patch: async <T = any>(url: string, data: any, config?: AxiosRequestConfig): Promise<T> => {
    const response: AxiosResponse<T> = await api.patch(url, data, config);
    return response.data;
  },

  delete: async <T = any>(url: string, config?: AxiosRequestConfig): Promise<T> => {
    const response: AxiosResponse<T> = await api.delete(url, config);
    return response.data;
  },

  getMediaUrl: (path: string): string => {
    if (!path) return '';
    if (path.startsWith('http://') || path.startsWith('https://')) return path;
    // Remove leading slash if present to avoid double slashes
    const cleanPath = path.startsWith('/') ? path.slice(1) : path;
    return `${BASE_URL}/${cleanPath}`;
  },

  /**
   * Fetches an authenticated endpoint as a PDF and opens it in a new tab.
   *
   * Used by the PDF export module. A plain `window.open(url)` would drop the
   * Authorization header and return 401, so the document is fetched as a blob
   * and opened through an object URL instead.
   */
  openPdf: async (url: string): Promise<void> => {
    const response = await api.get(url, { responseType: 'blob' });
    const blobUrl = URL.createObjectURL(
      new Blob([response.data], { type: 'application/pdf' })
    );
    window.open(blobUrl, '_blank');
    // The viewer keeps its own reference; this only releases the object URL.
    setTimeout(() => URL.revokeObjectURL(blobUrl), 60_000);
  },
};

export default apiService;
