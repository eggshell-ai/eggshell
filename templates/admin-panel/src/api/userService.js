import apiService from './apiService';

const userService = {
  getUsers: async (config = {}) => {
    return apiService.get('/users', config);
  },

  getUser: async (id, config = {}) => {
    return apiService.get(`/users/${id}`, config);
  },

  createUser: async (data, config = {}) => {
    return apiService.post('/users', data, config);
  },

  updateUser: async (id, data, config = {}) => {
    return apiService.put(`/users/${id}`, data, config);
  },

  deleteUser: async (id, config = {}) => {
    return apiService.delete(`/users/${id}`, config);
  },
};

export default userService;
