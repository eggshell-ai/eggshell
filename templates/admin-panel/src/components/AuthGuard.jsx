'use client';

import { useEffect, useState } from 'react';
import { useRouter } from 'next/navigation';
import authService from 'services/authService';

export default function AuthGuard({ children }) {
  const router = useRouter();
  const [isAuthenticated, setIsAuthenticated] = useState(false);
  const [isChecking, setIsChecking] = useState(true);

  useEffect(() => {
    const authenticated = authService.isAuthenticated();
    setIsAuthenticated(authenticated);
    setIsChecking(false);

    if (!authenticated) {
      router.push('/login');
    }
  }, [router]);

  if (isChecking) {
    return null;
  }

  if (!isAuthenticated) {
    return null;
  }

  return <>{children}</>;
}
