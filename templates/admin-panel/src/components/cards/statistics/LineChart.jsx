'use client';

import { useState, useEffect } from 'react';
import PropTypes from 'prop-types';

// material-ui
import CircularProgress from '@mui/material/CircularProgress';
import Grid from '@mui/material/Grid';
import Box from '@mui/material/Box';
import Typography from '@mui/material/Typography';

// @mui/x-charts
import { LineChart } from '@mui/x-charts/LineChart';

// project imports
import apiService from 'api/apiService';
import { useDashboardContext } from 'components/dashboard/DashboardContext';
import { parseGridSize } from 'components/dashboard/DashboardWidget';
import MainCard from 'components/MainCard';

export default function LineChartCard({
  title,
  endpoint,
  refreshTrigger: propRefreshTrigger,
  onCacheUpdate: propOnCacheUpdate,
  cacheTTL = 300000,
  size,
  xAxisKey = 'x',
  yAxisKey = 'y',
  seriesLabel = 'Value',
  height = 300,
  ...props
}) {
  const dashboard = useDashboardContext();
  const refreshTrigger = propRefreshTrigger ?? dashboard?.refreshTrigger;
  const onCacheUpdate = propOnCacheUpdate ?? dashboard?.reportCacheUpdate;

  const [data, setData] = useState(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);

  const cacheKey = endpoint ? `linechart_cache_${endpoint}` : null;

  // Register cache key with Dashboard context if present
  useEffect(() => {
    if (cacheKey && dashboard?.registerCacheKey) {
      dashboard.registerCacheKey(cacheKey);
    }
  }, [cacheKey, dashboard?.registerCacheKey]);

  useEffect(() => {
    let isMounted = true;

    const loadData = async () => {
      // Check cache if no refresh trigger has been activated
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

  // Transform data for chart
  const chartData = data ? {
    xAxis: data.xAxis || data.labels || data.map((item) => item[xAxisKey]),
    series: data.series || data.data || [{
      data: data.map((item) => item[yAxisKey]),
      label: seriesLabel
    }]
  } : null;

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
        ) : chartData ? (
          <Box sx={{ height }}>
            <LineChart
              xAxis={Array.isArray(chartData.xAxis) ? [{ data: chartData.xAxis }] : chartData.xAxis}
              series={Array.isArray(chartData.series) ? chartData.series : [chartData.series]}
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

LineChartCard.propTypes = {
  title: PropTypes.string.isRequired,
  endpoint: PropTypes.string.isRequired,
  refreshTrigger: PropTypes.any,
  onCacheUpdate: PropTypes.func,
  cacheTTL: PropTypes.number,
  size: PropTypes.oneOfType([PropTypes.number, PropTypes.object]),
  xAxisKey: PropTypes.string,
  yAxisKey: PropTypes.string,
  seriesLabel: PropTypes.string,
  height: PropTypes.number
};
