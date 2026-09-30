'use client';

import { useState, useEffect } from 'react';
import PropTypes from 'prop-types';

// material-ui
import CircularProgress from '@mui/material/CircularProgress';
import Grid from '@mui/material/Grid';
import Box from '@mui/material/Box';
import Typography from '@mui/material/Typography';

// @mui/x-charts
import { PieChart } from '@mui/x-charts/PieChart';

// project imports
import apiService from 'api/apiService';
import { useDashboardContext } from 'components/dashboard/DashboardContext';
import { parseGridSize } from 'components/dashboard/DashboardWidget';
import MainCard from 'components/MainCard';

export default function PieChartCard({
  title,
  endpoint,
  refreshTrigger: propRefreshTrigger,
  onCacheUpdate: propOnCacheUpdate,
  cacheTTL = 300000,
  size,
  innerRadius = 0,
  height = 300,
  ...props
}) {
  const dashboard = useDashboardContext();
  const refreshTrigger = propRefreshTrigger ?? dashboard?.refreshTrigger;
  const onCacheUpdate = propOnCacheUpdate ?? dashboard?.reportCacheUpdate;

  const [data, setData] = useState(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);

  const cacheKey = endpoint ? `piechart_cache_${endpoint}` : null;

  useEffect(() => {
    if (cacheKey && dashboard?.registerCacheKey) {
      dashboard.registerCacheKey(cacheKey);
    }
  }, [cacheKey, dashboard?.registerCacheKey]);

  useEffect(() => {
    let isMounted = true;

    const loadData = async () => {
      if (!propRefreshTrigger && !dashboard?.refreshTrigger) {
        try {
          const cached = localStorage.getItem(cacheKey);
          if (cached) {
            const { value, timestamp } = JSON.parse(cached);
            const isExpired = Date.now() - timestamp > cacheTTL;
            if (!isExpired) {
              if (isMounted) {
                setData(value);
                setLoading(false);
                setError(null);
                if (onCacheUpdate) onCacheUpdate(timestamp);
              }
              return;
            }
          }
        } catch {
          // If reading cache fails, proceed with fetch
        }
      }

      try {
        setLoading(true);
        const res = await apiService.get(endpoint);
        if (isMounted) {
          const timestamp = Date.now();
          try {
            localStorage.setItem(cacheKey, JSON.stringify({ value: res, timestamp }));
          } catch {
            // Ignore localStorage write error
          }
          setData(res);
          setError(null);
          if (onCacheUpdate) onCacheUpdate(timestamp);
        }
      } catch (err) {
        if (isMounted) {
          setError(err.message || 'Error loading data');
        }
      } finally {
        if (isMounted) {
          setLoading(false);
        }
      }
    };

    if (endpoint) {
      loadData();
    }
    return () => {
      isMounted = false;
    };
  }, [endpoint, refreshTrigger, cacheTTL, cacheKey]);

  // Format data for PieChart: expect array of { id, value, label } or { series: [...] } or { labels: [], data: [] }
  const pieData = data
    ? (() => {
        if (Array.isArray(data)) {
          return data.map((item, idx) => ({
            id: item.id ?? idx,
            value: Number(item.value ?? item.count ?? item.total ?? 0),
            label: String(item.label ?? item.status ?? item.name ?? `Item ${idx + 1}`),
          }));
        }
        if (Array.isArray(data.series)) {
          return data.series.map((item, idx) => ({
            id: item.id ?? idx,
            value: Number(item.value ?? item.count ?? 0),
            label: String(item.label ?? `Item ${idx + 1}`),
          }));
        }
        if (Array.isArray(data.labels) && Array.isArray(data.data)) {
          return data.labels.map((label, idx) => ({
            id: idx,
            value: Number(data.data[idx] ?? 0),
            label: String(label),
          }));
        }
        return [];
      })()
    : [];

  const content = (
    <MainCard content={false} {...props}>
      <Box sx={{ p: 2 }}>
        <Typography variant="h5" sx={{ mb: 2 }}>
          {title}
        </Typography>
        {loading ? (
          <Box sx={{ display: 'flex', justifyContent: 'center', alignItems: 'center', height }}>
            <CircularProgress />
          </Box>
        ) : error ? (
          <Box sx={{ display: 'flex', justifyContent: 'center', alignItems: 'center', height }}>
            <Typography color="error">{error}</Typography>
          </Box>
        ) : pieData.length > 0 ? (
          <Box sx={{ height }}>
            <PieChart
              series={[
                {
                  data: pieData,
                  innerRadius: innerRadius,
                  highlightScope: { fade: 'global', highlight: 'item' },
                },
              ]}
              height={height}
            />
          </Box>
        ) : (
          <Box sx={{ display: 'flex', justifyContent: 'center', alignItems: 'center', height }}>
            <Typography color="textSecondary">No data available</Typography>
          </Box>
        )}
      </Box>
    </MainCard>
  );

  if (size) {
    return <Grid size={parseGridSize(size)}>{content}</Grid>;
  }

  return content;
}

PieChartCard.propTypes = {
  title: PropTypes.string.isRequired,
  endpoint: PropTypes.string.isRequired,
  refreshTrigger: PropTypes.any,
  onCacheUpdate: PropTypes.func,
  cacheTTL: PropTypes.number,
  size: PropTypes.oneOfType([PropTypes.number, PropTypes.object]),
  innerRadius: PropTypes.number,
  height: PropTypes.number,
};
