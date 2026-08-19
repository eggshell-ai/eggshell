'use client';

import { createContext, useContext, useState, useRef, useCallback } from 'react';
import PropTypes from 'prop-types';

const DashboardContext = createContext(null);

export function DashboardProvider({ children }) {
  const [refreshTrigger, setRefreshTrigger] = useState(0);
  const [lastCachedDate, setLastCachedDate] = useState(null);
  const registeredCacheKeysRef = useRef(new Set());

  const registerCacheKey = useCallback((key) => {
    if (key) {
      registeredCacheKeysRef.current.add(key);
    }
  }, []);

  const reportCacheUpdate = useCallback((timestamp) => {
    if (timestamp) {
      setLastCachedDate((prev) => (!prev || timestamp > prev ? timestamp : prev));
    }
  }, []);

  const handleRefresh = useCallback(() => {
    try {
      // Invalidate all registered cache keys
      registeredCacheKeysRef.current.forEach((key) => {
        localStorage.removeItem(key);
      });

      // Sweep any localStorage item starting with kpi_cache_ for complete coverage
      if (typeof window !== 'undefined' && window.localStorage) {
        for (let i = 0; i < localStorage.length; i++) {
          const key = localStorage.key(i);
          if (key && key.startsWith('kpi_cache_')) {
            localStorage.removeItem(key);
          }
        }
      }
    } catch {
      // Ignore localStorage errors
    }

    setRefreshTrigger((prev) => prev + 1);
  }, []);

  return (
    <DashboardContext.Provider
      value={{
        refreshTrigger,
        lastCachedDate,
        registerCacheKey,
        reportCacheUpdate,
        handleRefresh
      }}
    >
      {children}
    </DashboardContext.Provider>
  );
}

DashboardProvider.propTypes = {
  children: PropTypes.node
};

export function useDashboardContext() {
  return useContext(DashboardContext);
}

export default DashboardContext;
