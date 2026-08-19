'use client';

import PropTypes from 'prop-types';

// material-ui
import Button from '@mui/material/Button';
import Grid from '@mui/material/Grid';
import Stack from '@mui/material/Stack';
import Typography from '@mui/material/Typography';

// assets
import ReloadOutlined from '@ant-design/icons/ReloadOutlined';

// project imports
import { DashboardProvider, useDashboardContext } from './DashboardContext';

function DashboardInner({ title = 'Dashboard', children, rowSpacing = 4.5, columnSpacing = 2.75 }) {
  const dashboard = useDashboardContext();
  const lastCachedDate = dashboard?.lastCachedDate;
  const handleRefresh = dashboard?.handleRefresh;

  return (
    <Grid container rowSpacing={rowSpacing} columnSpacing={columnSpacing}>
      {/* Dashboard Header */}
      <Grid size={12}>
        <Stack direction="row" sx={{ alignItems: 'center', justifyContent: 'space-between', flexWrap: 'wrap', gap: 2 }}>
          <Typography variant="h5">{title}</Typography>
          <Stack direction="row" sx={{ alignItems: 'center', gap: 2 }}>
            {lastCachedDate && (
              <Typography variant="caption" sx={{ color: 'text.secondary' }}>
                Last cached: {new Date(lastCachedDate).toLocaleString()}
              </Typography>
            )}
            <Button variant="outlined" size="small" startIcon={<ReloadOutlined />} onClick={handleRefresh}>
              Refresh
            </Button>
          </Stack>
        </Stack>
      </Grid>

      {/* Dashboard Widgets */}
      {children}
    </Grid>
  );
}

DashboardInner.propTypes = {
  title: PropTypes.string,
  children: PropTypes.node,
  rowSpacing: PropTypes.number,
  columnSpacing: PropTypes.number
};

export default function Dashboard({ title = 'Dashboard', children, rowSpacing = 4.5, columnSpacing = 2.75 }) {
  return (
    <DashboardProvider>
      <DashboardInner title={title} rowSpacing={rowSpacing} columnSpacing={columnSpacing}>
        {children}
      </DashboardInner>
    </DashboardProvider>
  );
}

Dashboard.propTypes = {
  title: PropTypes.string,
  children: PropTypes.node,
  rowSpacing: PropTypes.number,
  columnSpacing: PropTypes.number
};
