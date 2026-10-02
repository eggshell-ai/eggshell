'use client';

import React from 'react';
import PropTypes from 'prop-types';

// material-ui
import Button from '@mui/material/Button';
import Box from '@mui/material/Box';
import Stack from '@mui/material/Stack';
import Typography from '@mui/material/Typography';

// assets
import ReloadOutlined from '@ant-design/icons/ReloadOutlined';

// project imports
import { DashboardProvider, useDashboardContext } from './DashboardContext';

function getWidgetKind(child) {
  if (!child || !React.isValidElement(child)) return 'other';

  // Check static dashboardKind or prop kind
  const explicitKind = child.props?.dashboardKind || child.type?.dashboardKind || child.props?.kind;
  if (explicitKind) return explicitKind;

  const componentName = child.type?.displayName || child.type?.name || '';
  if (/kpi|analytic/i.test(componentName)) return 'kpi';
  if (/chart/i.test(componentName)) return 'chart';
  if (/table/i.test(componentName)) return 'table';

  return 'other';
}

function chunkArray(items, size) {
  const chunks = [];
  for (let i = 0; i < items.length; i += size) {
    chunks.push(items.slice(i, i + size));
  }
  return chunks;
}

function DashboardInner({ title = 'Dashboard', children, rowSpacing = 3, columnSpacing = 2.75 }) {
  const dashboard = useDashboardContext();
  const lastCachedDate = dashboard?.lastCachedDate;
  const handleRefresh = dashboard?.handleRefresh;

  // Flatten and classify children
  const rawChildren = React.Children.toArray(children).filter(Boolean);

  const kpiItems = [];
  const chartItems = [];
  const tableItems = [];
  const otherItems = [];

  rawChildren.forEach((child) => {
    const kind = getWidgetKind(child);
    if (kind === 'kpi') {
      kpiItems.push(child);
    } else if (kind === 'chart') {
      chartItems.push(child);
    } else if (kind === 'table') {
      tableItems.push(child);
    } else {
      otherItems.push(child);
    }
  });

  const gap = typeof columnSpacing === 'number' ? `${columnSpacing * 8}px` : columnSpacing;
  const rowGap = typeof rowSpacing === 'number' ? `${rowSpacing * 8}px` : rowSpacing;

  return (
    <Box sx={{ display: 'flex', flexDirection: 'column', gap: rowGap, width: '100%' }}>
      {/* Dashboard Header */}
      <Box sx={{ width: '100%' }}>
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
      </Box>

      {/* KPI Section: One row, equal columns (up to 4 per row, 3 per row beyond that) */}
      {kpiItems.length > 0 && (() => {
        const kpisPerRow = kpiItems.length <= 4 ? kpiItems.length : 3;
        const kpiChunks = chunkArray(kpiItems, kpisPerRow);

        return kpiChunks.map((chunk, chunkIdx) => (
          <Box
            key={`kpi-row-${chunkIdx}`}
            sx={{
              display: 'grid',
              gap,
              alignItems: 'stretch',
              gridTemplateColumns: {
                xs: '1fr',
                sm: chunk.length > 1 ? 'repeat(2, 1fr)' : '1fr',
                md: `repeat(${chunk.length}, 1fr)`
              },
              '& > *': {
                height: '100%',
                minWidth: 0
              }
            }}
          >
            {chunk.map((item, itemIdx) => (
              <Box key={`kpi-${chunkIdx}-${itemIdx}`} sx={{ height: '100%', minWidth: 0, '& > *': { height: '100%' } }}>
                {item}
              </Box>
            ))}
          </Box>
        ));
      })()}

      {/* Charts Section: 2 per row; an odd last chart spans the full width */}
      {chartItems.length > 0 && (() => {
        const chartChunks = chunkArray(chartItems, 2);

        return chartChunks.map((chunk, chunkIdx) => (
          <Box
            key={`chart-row-${chunkIdx}`}
            sx={{
              display: 'grid',
              gap,
              alignItems: 'stretch',
              gridTemplateColumns: {
                xs: '1fr',
                md: chunk.length === 1 ? '1fr' : 'repeat(2, 1fr)'
              },
              '& > *': {
                height: '100%',
                minWidth: 0
              }
            }}
          >
            {chunk.map((item, itemIdx) => (
              <Box key={`chart-${chunkIdx}-${itemIdx}`} sx={{ height: '100%', minWidth: 0, '& > *': { height: '100%' } }}>
                {item}
              </Box>
            ))}
          </Box>
        ));
      })()}

      {/* Tables Section: 2 per row; an odd last table spans the full width */}
      {tableItems.length > 0 && (() => {
        const tableChunks = chunkArray(tableItems, 2);

        return tableChunks.map((chunk, chunkIdx) => (
          <Box
            key={`table-row-${chunkIdx}`}
            sx={{
              display: 'grid',
              gap,
              alignItems: 'stretch',
              gridTemplateColumns: {
                xs: '1fr',
                md: chunk.length === 1 ? '1fr' : 'repeat(2, 1fr)'
              },
              '& > *': {
                height: '100%',
                minWidth: 0
              }
            }}
          >
            {chunk.map((item, itemIdx) => (
              <Box key={`table-${chunkIdx}-${itemIdx}`} sx={{ height: '100%', minWidth: 0, '& > *': { height: '100%' } }}>
                {item}
              </Box>
            ))}
          </Box>
        ));
      })()}

      {/* Other Items */}
      {otherItems.length > 0 && (
        <Box
          sx={{
            display: 'grid',
            gap,
            alignItems: 'stretch',
            gridTemplateColumns: {
              xs: '1fr',
              md: 'repeat(auto-fit, minmax(320px, 1fr))'
            },
            '& > *': {
              height: '100%',
              minWidth: 0
            }
          }}
        >
          {otherItems.map((item, itemIdx) => (
            <Box key={`other-${itemIdx}`} sx={{ height: '100%', minWidth: 0, '& > *': { height: '100%' } }}>
              {item}
            </Box>
          ))}
        </Box>
      )}
    </Box>
  );
}

DashboardInner.propTypes = {
  title: PropTypes.string,
  children: PropTypes.node,
  rowSpacing: PropTypes.oneOfType([PropTypes.number, PropTypes.string]),
  columnSpacing: PropTypes.oneOfType([PropTypes.number, PropTypes.string])
};

export default function Dashboard({ title = 'Dashboard', children, rowSpacing = 3, columnSpacing = 2.75 }) {
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
  rowSpacing: PropTypes.oneOfType([PropTypes.number, PropTypes.string]),
  columnSpacing: PropTypes.oneOfType([PropTypes.number, PropTypes.string])
};
