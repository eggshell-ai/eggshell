// project import
import { DashboardOutlined, UserOutlined, TeamOutlined, AppstoreOutlined } from '@ant-design/icons';
import authService from '../services/authService';
import menuData from '../../schemas/menu.json';

// icons
const icons = {
  DashboardOutlined,
  UserOutlined,
  TeamOutlined,
  AppstoreOutlined
};

// ==============================|| MENU GENERATION FROM JSON ||============================== //

const buildMenuStructure = (menuItems) => {
  const groups = {};

  menuItems.forEach(item => {
    const parts = item.name.split('.');
    if (parts.length < 2) return;

    const groupName = parts[0];
    const itemName = parts.slice(1).join(' ');

    if (!groups[groupName]) {
      groups[groupName] = {
        id: `group-${groupName.toLowerCase()}`,
        title: groupName.charAt(0).toUpperCase() + groupName.slice(1),
        type: 'group',
        children: []
      };
    }

    // Get icon from JSON data, fallback to DashboardOutlined if not found
    const iconName = item.icon || 'DashboardOutlined';
    const iconComponent = icons[iconName] || icons.DashboardOutlined;

    groups[groupName].children.push({
      id: itemName.toLowerCase().replace(/\s+/g, '-'),
      title: itemName,
      type: 'item',
      url: item.route,
      icon: iconComponent,
      breadcrumbs: false,
      ...(item.permission && { permission: item.permission })
    });
  });

  return Object.values(groups);
};

// ==============================|| PERMISSION FILTERING ||============================== //

const filterMenuItem = async (item) => {
  if (item.type === 'group' && item.children) {
    const filteredChildren = [];
    for (const child of item.children) {
      const filteredChild = await filterMenuItem(child);
      if (filteredChild !== null) {
        filteredChildren.push(filteredChild);
      }
    }

    if (filteredChildren.length === 0) {
      return null;
    }

    return {
      ...item,
      children: filteredChildren
    };
  }

  if (item.permission) {
    const hasAccess = await authService.hasPermission(item.permission);
    if (!hasAccess) {
      return null;
    }
  }

  return item;
};

const getMenuItems = async () => {
  try {
    const menuStructure = buildMenuStructure(menuData);
    
    const filteredItems = [];
    for (const item of menuStructure) {
      const filteredItem = await filterMenuItem(item);
      if (filteredItem !== null) {
        filteredItems.push(filteredItem);
      }
    }

    return {
      items: filteredItems
    };
  } catch (error) {
    console.error('Error loading menu from JSON:', error);
    return { items: [] };
  }
};

// ==============================|| MENU ITEMS ||============================== //

const menuItems = {
  items: []
};

export default menuItems;
export { getMenuItems };
