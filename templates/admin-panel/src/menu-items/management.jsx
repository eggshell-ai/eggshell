// assets
import { UserOutlined, TeamOutlined } from '@ant-design/icons';

// icons
const icons = {
  UserOutlined,
  TeamOutlined
};

// ==============================|| MENU ITEMS - MANAGEMENT ||============================== //

const management = {
  id: 'group-management',
  title: 'Management',
  type: 'group',
  children: [
    {
      id: 'users',
      title: 'Users',
      type: 'item',
      url: '/users',
      icon: icons.UserOutlined,
      breadcrumbs: false,
      permission: 'users.index'
    },
    {
      id: 'roles',
      title: 'Roles',
      type: 'item',
      url: '/roles',
      icon: icons.TeamOutlined,
      breadcrumbs: false,
      permission: 'roles.index'
    }
  ]
};

export default management;
