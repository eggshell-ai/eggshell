'use client';

import ResourcePage from '@/components/resources/ResourcePage';
import rolesResource from '@/resources/roles';

export default function Roles() {
  return <ResourcePage resource={rolesResource} />;
}
