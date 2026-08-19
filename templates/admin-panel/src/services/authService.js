import axios from 'axios';

const API_URL = process.env.NEXT_PUBLIC_API_URL || 'http://localhost:8000/api';

const TOKEN_KEY = 'token';
const USER_KEY = 'user';
const PERMISSIONS_KEY = 'permissions';

// Create axios instance for auth service
const authApi = axios.create({
  baseURL: API_URL,
  headers: {
    'Content-Type': 'application/json',
  },
});

// Request interceptor to add auth token if available
authApi.interceptors.request.use(
  (config) => {
    const token = localStorage.getItem(TOKEN_KEY);
    if (token) {
      config.headers.Authorization = `Bearer ${token}`;
    }
    return config;
  },
  (error) => {
    return Promise.reject(error);
  }
);

// Response interceptor to handle errors
authApi.interceptors.response.use(
  (response) => response,
  (error) => {
    if (error.response?.status === 401) {
      // Handle unauthorized access
      localStorage.removeItem(TOKEN_KEY);
      localStorage.removeItem(USER_KEY);
      window.location.href = '/login';
    }
    return Promise.reject(error);
  }
);

const authService = {
  // Check if user is authenticated
  isAuthenticated: () => {
    if (typeof window === 'undefined') return false;
    const token = localStorage.getItem(TOKEN_KEY);
    return !!token;
  },

  // Get the auth token
  getToken: () => {
    if (typeof window === 'undefined') return null;
    return localStorage.getItem(TOKEN_KEY);
  },

  // Get the current user
  getUser: () => {
    if (typeof window === 'undefined') return null;
    const userStr = localStorage.getItem(USER_KEY);
    if (userStr) {
      try {
        return JSON.parse(userStr);
      } catch (e) {
        return null;
      }
    }
    return null;
  },

  // Login with email and password
  login: async (email, password) => {
    const response = await authApi.post('/login', { email, password });
    
    if (response.data.token) {
      localStorage.setItem(TOKEN_KEY, response.data.token);
      
      if (response.data.user) {
        localStorage.setItem(USER_KEY, JSON.stringify(response.data.user));
      }
    }
    
    return response.data;
  },

  // Logout the user
  logout: () => {
    localStorage.removeItem(TOKEN_KEY);
    localStorage.removeItem(USER_KEY);
    authService.clearPermissionsCache();
  },

  // Set auth data manually (useful if you receive token from elsewhere)
  setAuthData: (token, user = null) => {
    localStorage.setItem(TOKEN_KEY, token);
    if (user) {
      localStorage.setItem(USER_KEY, JSON.stringify(user));
    }
  },

  // Get user permissions (cached)
  getPermissions: async () => {
    // Check if permissions are cached in memory
    if (authService._cachedPermissions) {
      return authService._cachedPermissions;
    }

    // If a request is already in flight, return the existing promise
    if (authService._permissionsPromise) {
      return authService._permissionsPromise;
    }

    authService._permissionsPromise = (async () => {
      try {
        const response = await authApi.get('/permissions/my');
        // If role is ADMIN, give all permissions
        let permissions;
        if (response.data.role === 'ADMIN') {
          permissions = { role: 'ADMIN', permissions: ['*'] };
        } else {
          permissions = response.data;
        }
        authService._cachedPermissions = permissions;
        return permissions;
      } catch (error) {
        console.error('Failed to load permissions:', error);
        return { role: null, permissions: [] };
      } finally {
        authService._permissionsPromise = null;
      }
    })();

    return authService._permissionsPromise;
  },

  // Check if user has a specific permission
  hasPermission: async (permission) => {
    const permissionsData = await authService.getPermissions();
    const availablePermissions = permissionsData.permissions || [];

    // If no permission required, allow access
    if (!permission) return true;

    // If wildcard is present, allow all permissions
    if (availablePermissions.includes('*')) return true;

    // Check if the specific permission is available
    return availablePermissions.includes(permission);
  },

  // Clear cached permissions (useful after logout or role changes)
  clearPermissionsCache: () => {
    authService._cachedPermissions = null;
    authService._permissionsPromise = null;
  }
};

export default authService;
