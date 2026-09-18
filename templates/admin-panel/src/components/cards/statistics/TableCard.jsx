'use client';

import { useState, useEffect } from 'react';
import PropTypes from 'prop-types';
import Link from 'next/link';

// material-ui
import CircularProgress from '@mui/material/CircularProgress';
import Grid from '@mui/material/Grid';
import Box from '@mui/material/Box';
import Typography from '@mui/material/Typography';
import Table from '@mui/material/Table';
import TableBody from '@mui/material/TableBody';
import TableCell from '@mui/material/TableCell';
import TableContainer from '@mui/material/TableContainer';
import TableHead from '@mui/material/TableHead';
import TableRow from '@mui/material/TableRow';

// project imports
import apiService from 'api/apiService';
import { useDashboardContext } from 'components/dashboard/DashboardContext';
import { parseGridSize } from 'components/dashboard/DashboardWidget';
import MainCard from 'components/MainCard';

export default function TableCard({
  title,
  endpoint,
  columns = [],
  refreshTrigger: propRefreshTrigger,
  onCacheUpdate: propOnCacheUpdate,
  cacheTTL = 300000,
  size,
  maxRows,
  ...props
}) {
  const dashboard = useDashboardContext();
  const refreshTrigger = propRefreshTrigger ?? dashboard?.refreshTrigger;
  const onCacheUpdate = propOnCacheUpdate ?? dashboard?.reportCacheUpdate;

  const [data, setData] = useState([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);

  const cacheKey = endpoint ? `tablecard_cache_${endpoint}` : null;

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
          const items = Array.isArray(res) ? res : (res?.data || res?.items || []);
          const timestamp = Date.now();
          try {
            localStorage.setItem(cacheKey, JSON.stringify({ value: items, timestamp }));
          } catch {
            // Ignore localStorage write error
          }
          setData(items);
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

  const rows = maxRows && Array.isArray(data) ? data.slice(0, maxRows) : (Array.isArray(data) ? data : []);

  const renderCellContent = (row, col) => {
    const rawVal = row[col.dataIndex || col.field || col.key];
    let content = col.render ? col.render(rawVal, row) : rawVal !== undefined && rawVal !== null ? String(rawVal) : '-';

    if (col.link) {
      // Replace {field} template in link string, e.g. "/products/{id}"
      const href = col.link.replace(/\{(\w+)\}/g, (_, fieldName) => row[fieldName] ?? '');
      return (
        <Link href={href} style={{ textDecoration: 'none', color: '#1890ff', fontWeight: 500 }}>
          {content}
        </Link>
      );
    }

    return content;
  };

  const content = (
    <MainCard content={false} {...props}>
      <Box sx={{ p: 2 }}>
        <Typography variant="h5" sx={{ mb: 2 }}>
          {title}
        </Typography>
        {loading ? (
          <Box sx={{ display: 'flex', justifyContent: 'center', alignItems: 'center', minHeight: 200 }}>
            <CircularProgress />
          </Box>
        ) : error ? (
          <Box sx={{ display: 'flex', justifyContent: 'center', alignItems: 'center', minHeight: 200 }}>
            <Typography color="error">{error}</Typography>
          </Box>
        ) : rows.length > 0 ? (
          <TableContainer>
            <Table size="small">
              <TableHead>
                <TableRow>
                  {columns.map((col, idx) => (
                    <TableCell key={col.dataIndex || col.key || idx} align={col.align || 'left'} sx={{ fontWeight: 600 }}>
                      {col.title || col.label || col.header}
                    </TableCell>
                  ))}
                </TableRow>
              </TableHead>
              <TableBody>
                {rows.map((row, rowIdx) => (
                  <TableRow key={row.id ?? rowIdx} hover>
                    {columns.map((col, colIdx) => (
                      <TableCell key={col.dataIndex || col.key || colIdx} align={col.align || 'left'}>
                        {renderCellContent(row, col)}
                      </TableCell>
                    ))}
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </TableContainer>
        ) : (
          <Box sx={{ display: 'flex', justifyContent: 'center', alignItems: 'center', minHeight: 150 }}>
            <Typography color="textSecondary">No records found</Typography>
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

TableCard.propTypes = {
  title: PropTypes.string.isRequired,
  endpoint: PropTypes.string.isRequired,
  columns: PropTypes.array.isRequired,
  refreshTrigger: PropTypes.any,
  onCacheUpdate: PropTypes.func,
  cacheTTL: PropTypes.number,
  size: PropTypes.oneOfType([PropTypes.number, PropTypes.object]),
  maxRows: PropTypes.number,
};
