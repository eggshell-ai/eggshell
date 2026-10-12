'use client';

import { useState, useEffect, useMemo } from 'react';
import PropTypes from 'prop-types';

// material-ui
import { useTheme } from '@mui/material/styles';
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
  innerRadius = 60,
  height = 280,
  sx,
  ...props
}) {
  const theme = useTheme();
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
  const rawPieData = useMemo(() => {
    if (!data) return [];
    if (Array.isArray(data)) {
      return data.map((item, idx) => ({
        id: item.id ?? idx,
        value: Number(item.value ?? item.count ?? item.total ?? 0),
        label: String(item.label ?? item.status ?? item.name ?? `Item ${idx + 1}`),
        ...(item.color ? { color: item.color } : {}),
      }));
    }
    if (Array.isArray(data.series)) {
      return data.series.map((item, idx) => ({
        id: item.id ?? idx,
        value: Number(item.value ?? item.count ?? 0),
        label: String(item.label ?? `Item ${idx + 1}`),
        ...(item.color ? { color: item.color } : {}),
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
  }, [data]);

  const activeSlices = useMemo(() => rawPieData.filter((item) => item.value > 0), [rawPieData]);
  const totalValue = useMemo(() => activeSlices.reduce((sum, item) => sum + item.value, 0), [activeSlices]);
  const hasData = rawPieData.length > 0 && totalValue > 0;
  const isSingleSlice = hasData && activeSlices.length === 1;
  const singleSlice = isSingleSlice ? activeSlices[0] : null;

  // Donut radius
  const donutRadius = typeof innerRadius === 'number' && innerRadius > 0
    ? innerRadius
    : Math.min(60, Math.round(height * 0.25));

  // Neutral placeholder segments for no data
  const isDark = theme?.palette?.mode === 'dark';
  const placeholderColors = useMemo(
    () =>
      isDark
        ? ['#383838', '#2d2d2d', '#444444', '#262626']
        : [
            theme?.palette?.grey?.[300] || '#d9d9d9',
            theme?.palette?.grey?.[200] || '#e8e8e8',
            theme?.palette?.grey?.[100] || '#f2f2f2',
            theme?.palette?.grey?.[200] || '#e0e0e0',
          ],
    [isDark, theme?.palette?.grey]
  );

  const placeholderData = useMemo(
    () => [
      { id: 'p1', value: 1, color: placeholderColors[0] },
      { id: 'p2', value: 1, color: placeholderColors[1] },
      { id: 'p3', value: 1, color: placeholderColors[2] },
      { id: 'p4', value: 1, color: placeholderColors[3] },
    ],
    [placeholderColors]
  );

  const content = (
    <MainCard content={false} sx={{ height: '100%', display: 'flex', flexDirection: 'column', ...(typeof sx === 'function' ? sx : sx || {}) }} {...props}>
      <Box sx={{ p: 2, flexGrow: 1, display: 'flex', flexDirection: 'column' }}>
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
        ) : !hasData ? (
          <Box sx={{ position: 'relative', height, width: '100%', display: 'flex', justifyContent: 'center', alignItems: 'center' }}>
            <PieChart
              series={[
                {
                  data: placeholderData,
                  innerRadius: donutRadius,
                  highlightScope: { fade: 'none', highlight: 'none' },
                },
              ]}
              height={height}
              margin={{ top: 10, bottom: 10, left: 10, right: 10 }}
              slotProps={{ legend: { hidden: true } }}
              tooltip={{ trigger: 'none' }}
            />
            <Box
              sx={{
                position: 'absolute',
                top: '50%',
                left: '50%',
                transform: 'translate(-50%, -50%)',
                textAlign: 'center',
                pointerEvents: 'none',
              }}
            >
              <Typography variant="body2" sx={{ color: 'text.secondary', fontWeight: 500 }}>
                No data
              </Typography>
            </Box>
          </Box>
        ) : isSingleSlice ? (
          <Box sx={{ position: 'relative', height, width: '100%', display: 'flex', justifyContent: 'center', alignItems: 'center' }}>
            <PieChart
              series={[
                {
                  data: [
                    {
                      id: singleSlice.id,
                      value: singleSlice.value,
                      label: singleSlice.label,
                      ...(singleSlice.color ? { color: singleSlice.color } : {}),
                    },
                  ],
                  innerRadius: donutRadius,
                  highlightScope: { fade: 'global', highlight: 'item' },
                },
              ]}
              height={height}
              margin={{ top: 10, bottom: 10, left: 10, right: 10 }}
              slotProps={{ legend: { hidden: true } }}
            />
            <Box
              sx={{
                position: 'absolute',
                top: '50%',
                left: '50%',
                transform: 'translate(-50%, -50%)',
                textAlign: 'center',
                pointerEvents: 'none',
                px: 1,
                maxWidth: Math.max(100, donutRadius * 2 - 12),
              }}
            >
              <Typography
                variant="subtitle2"
                sx={{
                  color: 'text.primary',
                  fontWeight: 600,
                  lineHeight: 1.2,
                  display: '-webkit-box',
                  WebkitLineClamp: 2,
                  WebkitBoxOrient: 'vertical',
                  overflow: 'hidden',
                }}
                title={singleSlice.label}
              >
                {singleSlice.label}
              </Typography>
              <Typography
                variant="caption"
                sx={{
                  color: 'text.secondary',
                  fontWeight: 700,
                  fontSize: '0.85rem',
                  lineHeight: 1.2,
                  display: 'block',
                  mt: 0.5,
                }}
              >
                100%
              </Typography>
            </Box>
          </Box>
        ) : (
          <Box sx={{ height, width: '100%' }}>
            <PieChart
              series={[
                {
                  data: activeSlices,
                  innerRadius: donutRadius,
                  highlightScope: { fade: 'global', highlight: 'item' },
                },
              ]}
              height={height}
            />
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

PieChartCard.dashboardKind = 'chart';

PieChartCard.propTypes = {
  title: PropTypes.string.isRequired,
  endpoint: PropTypes.string.isRequired,
  refreshTrigger: PropTypes.any,
  onCacheUpdate: PropTypes.func,
  cacheTTL: PropTypes.number,
  size: PropTypes.oneOfType([PropTypes.number, PropTypes.object]),
  innerRadius: PropTypes.number,
  height: PropTypes.number,
  sx: PropTypes.oneOfType([PropTypes.object, PropTypes.func]),
};
