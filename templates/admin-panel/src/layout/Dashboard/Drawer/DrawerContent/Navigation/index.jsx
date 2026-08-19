'use client';

import { useEffect, useState } from 'react';
// material-ui
import Typography from '@mui/material/Typography';
import Box from '@mui/material/Box';
import CircularProgress from '@mui/material/CircularProgress';

// project import
import NavGroup from './NavGroup';
import { getMenuItems } from 'menu-items';

// ==============================|| DRAWER CONTENT - NAVIGATION ||============================== //

export default function Navigation() {
  const [menuItems, setMenuItems] = useState(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const loadMenuItems = async () => {
      try {
        const data = await getMenuItems();
        setMenuItems(data.items);
      } catch (error) {
        console.error('Failed to load menu items:', error);
        setMenuItems([]);
      } finally {
        setLoading(false);
      }
    };

    loadMenuItems();
  }, []);

  if (loading) {
    return (
      <Box sx={{ pt: 2, display: 'flex', justifyContent: 'center' }}>
        <CircularProgress size={24} />
      </Box>
    );
  }

  const navGroups = menuItems?.map((item) => {
    switch (item.type) {
      case 'group':
        return <NavGroup key={item.id} item={item} />;
      default:
        return (
          <Typography key={item.id} variant="h6" sx={{ color: 'error.main', textAlign: 'center' }}>
            Fix - Navigation Group
          </Typography>
        );
    }
  }) || [];

  return <Box sx={{ pt: 2 }}>{navGroups}</Box>;
}
