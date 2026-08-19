import defineResource from '../utils/defineResource';
import field from '../utils/field';
import roleService from '../api/roleService';
import permissionsResource from './permissions';

export default defineResource({
  name: "roles",
  service: roleService,
  permissions: {
    'view': 'roles.view',
    'create': 'roles.create',
    'edit': 'roles.edit',
    'delete': 'roles.delete'
  },
  fields: [
    field.text("name")
      .label("Role Name")
      .table()
      .form()
      .required(),

    field.tags("permissions")
      .label("Permissions")
      .resource(permissionsResource)
      .table()
      .form()
  ]
});
