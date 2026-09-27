import apiService from './apiService';

/**
 * Service to interact with backend notification endpoints.
 */
const notificationService = {
  /**
   * Fetch active notifications and unread count for current user
   * @param {number} [limit=20]
   */
  getNotifications: async (limit = 20) => {
    return apiService.get(`/notifications?limit=${limit}`);
  },

  /**
   * Mark a specific notification as read
   * @param {number} id
   */
  markAsRead: async (id) => {
    return apiService.post(`/notifications/${id}/read`);
  },

  /**
   * Mark all notifications as read for current user
   */
  markAllAsRead: async () => {
    return apiService.post('/notifications/read-all');
  },

  /**
   * Create/dispatch a notification
   */
  createNotification: async (payload) => {
    return apiService.post('/notifications', payload);
  }
};

export default notificationService;
