'use client';

import ResourcePage from '@/components/resources/ResourcePage';
import usersResource from '@/resources/users';

export default function Users() {
  return <ResourcePage resource={usersResource} />;
}
