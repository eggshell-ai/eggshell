'use client';

import PropTypes from 'prop-types';

// project imports
import DashboardLayout from 'layout/Dashboard';
import AuthGuard from 'components/AuthGuard';

// ==============================|| DASHBOARD LAYOUT ||============================== //

export default function Layout({ children }) {
  return (
    <AuthGuard>
      <DashboardLayout>{children}</DashboardLayout>
    </AuthGuard>
  );
}

Layout.propTypes = { children: PropTypes.node };
