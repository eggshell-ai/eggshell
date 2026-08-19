import defineResource from '../utils/defineResource';
import field from '../utils/field';
import userService from '../api/userService';
import rolesResource from './roles';

export default defineResource({
  name: "users",
  service: userService,
  permissions: {
    'view': 'users.view',
    'create': 'usrers.create',
    'edit': 'users.edit',
    'delete': 'users.delete'
  },
  fields: [
    field.text("first_name")
      .label("First Name")
      .table()
      .form()
      .required(),
    
    field.text("last_name")
      .label("Last Name")
      .table()
      .form()
      .required(),

    field.email("email")
      .table()
      .form(),

    field.password('password')
      .form(),

    field.select("role")
      .resource(rolesResource)
      .table()
      .form()
  ]
});