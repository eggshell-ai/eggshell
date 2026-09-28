'use client';

import React from 'react';
import ResourcePage from '@/components/resources/ResourcePage';
import profileResource from '@/resources/profile';

export default function ProfilePage() {
  return <ResourcePage resource={profileResource} />;
}
