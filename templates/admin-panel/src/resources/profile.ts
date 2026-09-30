import defineResource from '../utils/defineResource';
import field from '../utils/field';
import profileService from '../api/profileService';

export default defineResource({
  name: 'profile',
  service: profileService,
  endpoint: '/profile',
  permissions: {
    view: 'profile.view',
    create: 'profile.create',
    edit: 'profile.edit',
    delete: 'profile.delete'
  },
  fields: [
    field.file('avatar')
      .label('Avatar')
      .accept('image/*')
      .table()
      .form()
  ]
});
