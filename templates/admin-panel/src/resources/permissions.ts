import defineResource from '../utils/defineResource';
import field from '../utils/field';

export default defineResource({
  name: "permissions",
  endpoint: "/permissions",
  permissions: {
    'view': 'permissions.view',
    'create': 'permissions.create',
    'edit': 'permissions.edit',
    'delete': 'permissions.delete'
  },
  fields: [
    field.text("name")
      .label("Permission Name")
      .table()
      .form()
      .required(),

    field.text("description")
      .label("Description")
      .table()
      .form()
  ]
});
