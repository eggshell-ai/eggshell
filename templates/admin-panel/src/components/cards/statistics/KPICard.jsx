'use client';

import { useState, useEffect } from 'react';
import PropTypes from 'prop-types';

// material-ui
import CircularProgress from '@mui/material/CircularProgress';
import Grid from '@mui/material/Grid';

// project imports
import AnalyticEcommerce from 'components/cards/statistics/AnalyticEcommerce';
import apiService from 'api/apiService';
import { useDashboardContext } from 'components/dashboard/DashboardContext';
import { parseGridSize } from 'components/dashboard/DashboardWidget';

export default function KPICard({
  title,
  endpoint,
  refreshTrigger: propRefreshTrigger,
  onCacheUpdate: propOnCacheUpdate,
  cacheTTL = 300000,
  size,
  icon,
  color,
  ...props
}) {
  const dashboard = useDashboardContext();
  const refreshTrigger = propRefreshTrigger ?? dashboard?.refreshTrigger;
  const onCacheUpdate = propOnCacheUpdate ?? dashboard?.reportCacheUpdate;

  const [data, setData] = useState(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);

  const cacheKey = endpoint ? `kpi_cache_${endpoint}` : null;

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
          const value = typeof res === 'object' && res !== null ? (res.count ?? res.value ?? JSON.stringify(res)) : res;
          const timestamp = Date.now();
          try {
            localStorage.setItem(cacheKey, JSON.stringify({ value, timestamp }));
          } catch {
            // Ignore localStorage write error
          }
          setData(value);
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

  let content;
  if (loading) {
    content = <AnalyticEcommerce title={title} count={<CircularProgress size={20} />} icon={icon} color={color} {...props} />;
  } else if (error) {
    content = <AnalyticEcommerce title={title} count="—" extra={error} icon={icon} color={color} {...props} />;
  } else {
    content = <AnalyticEcommerce title={title} count={data !== null ? String(data) : '—'} icon={icon} color={color} {...props} />;
  }

  if (size) {
    return <Grid size={parseGridSize(size)}>{content}</Grid>;
  }

  return content;
}

KPICard.propTypes = {
  title: PropTypes.string.isRequired,
  endpoint: PropTypes.string.isRequired,
  refreshTrigger: PropTypes.any,
  onCacheUpdate: PropTypes.func,
  cacheTTL: PropTypes.number,
  size: PropTypes.oneOfType([PropTypes.number, PropTypes.object]),
  icon: PropTypes.node,
  color: PropTypes.string
};
