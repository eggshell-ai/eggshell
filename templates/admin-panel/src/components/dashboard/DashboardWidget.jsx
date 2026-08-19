'use client';

import PropTypes from 'prop-types';
import Grid from '@mui/material/Grid';

export function parseGridSize(size) {
  if (!size) return { xs: 12, sm: 6, lg: 3 };
  if (typeof size === 'number') {
    if (size >= 12) return 12;
    return { xs: 12, sm: Math.min(size * 2, 12), lg: size };
  }
  return size;
}

export default function DashboardWidget({ size, children, ...props }) {
  const gridSize = parseGridSize(size);
  return (
    <Grid size={gridSize} {...props}>
      {children}
    </Grid>
  );
}

DashboardWidget.propTypes = {
  size: PropTypes.oneOfType([PropTypes.number, PropTypes.object]),
  children: PropTypes.node
};
