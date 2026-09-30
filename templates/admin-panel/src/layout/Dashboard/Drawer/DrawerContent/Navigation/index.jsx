'use client';

import { useEffect, useState, useMemo } from 'react';
// material-ui
import Typography from '@mui/material/Typography';
import Box from '@mui/material/Box';
import CircularProgress from '@mui/material/CircularProgress';
import OutlinedInput from '@mui/material/OutlinedInput';
import InputAdornment from '@mui/material/InputAdornment';
import IconButton from '@mui/material/IconButton';
import Tooltip from '@mui/material/Tooltip';

// assets
import SearchOutlined from '@ant-design/icons/SearchOutlined';
import CloseCircleOutlined from '@ant-design/icons/CloseCircleOutlined';

// project import
import NavGroup from './NavGroup';
import { getMenuItems } from 'menu-items';
import { handlerDrawerOpen, useGetMenuMaster } from 'api/menu';

// ==============================|| DRAWER CONTENT - NAVIGATION ||============================== //

export default function Navigation() {
  const [menuItems, setMenuItems] = useState(null);
  const [loading, setLoading] = useState(true);
  const [searchTerm, setSearchTerm] = useState('');

  const { menuMaster } = useGetMenuMaster();
  const drawerOpen = menuMaster.isDashboardDrawerOpened;

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

  // Filter menu items by search query
  const filteredMenuItems = useMemo(() => {
    if (!menuItems) return [];
    const query = searchTerm.trim().toLowerCase();
    if (!query) return menuItems;

    return menuItems
      .map((group) => {
        if (group.type !== 'group') return group;

        const groupTitleMatch = group.title?.toLowerCase().includes(query);
        const matchingChildren = group.children?.filter((child) => {
          return child.title?.toLowerCase().includes(query);
        }) || [];

        // If the group title matches, show all children in that group, otherwise show only matching items
        if (groupTitleMatch) {
          return group;
        }

        if (matchingChildren.length > 0) {
          return {
            ...group,
            children: matchingChildren
          };
        }

        return null;
      })
      .filter(Boolean);
  }, [menuItems, searchTerm]);

  if (loading) {
    return (
      <Box sx={{ pt: 2, display: 'flex', justifyContent: 'center' }}>
        <CircularProgress size={24} />
      </Box>
    );
  }

  const navGroups = filteredMenuItems?.map((item) => {
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

  return (
    <Box sx={{ pt: 1.5 }}>
      {/* Search Bar / Icon */}
      {drawerOpen ? (
        <Box sx={{ px: 2, mb: 1 }}>
          <OutlinedInput
            size="small"
            fullWidth
            value={searchTerm}
            onChange={(e) => setSearchTerm(e.target.value)}
            placeholder="Search menu..."
            startAdornment={
              <InputAdornment position="start" sx={{ mr: 0.75, color: 'text.secondary' }}>
                <SearchOutlined />
              </InputAdornment>
            }
            endAdornment={
              searchTerm ? (
                <InputAdornment position="end">
                  <IconButton
                    size="small"
                    edge="end"
                    aria-label="clear search"
                    onClick={() => setSearchTerm('')}
                    sx={{ color: 'text.secondary', p: 0.25 }}
                  >
                    <CloseCircleOutlined style={{ fontSize: '0.85rem' }} />
                  </IconButton>
                </InputAdornment>
              ) : null
            }
            sx={{
              bgcolor: 'background.paper',
              '& .MuiOutlinedInput-input': {
                py: 0.75,
                px: 0.5,
                fontSize: '0.8125rem'
              }
            }}
          />
        </Box>
      ) : (
        <Box sx={{ display: 'flex', justifyContent: 'center', mb: 1 }}>
          <Tooltip title="Search menu" placement="right">
            <IconButton
              size="small"
              color="secondary"
              onClick={() => handlerDrawerOpen(true)}
              sx={{
                width: 36,
                height: 36,
                borderRadius: 1.5,
                color: 'text.secondary',
                '&:hover': { bgcolor: 'secondary.lighter', color: 'primary.main' }
              }}
            >
              <SearchOutlined />
            </IconButton>
          </Tooltip>
        </Box>
      )}

      {/* Navigation Groups or Empty State */}
      {navGroups.length > 0 ? (
        navGroups
      ) : (
        <Box sx={{ px: 2.5, py: 2, textAlign: 'center' }}>
          <Typography variant="caption" sx={{ color: 'text.secondary' }}>
            No menu items found
          </Typography>
        </Box>
      )}
    </Box>
  );
}
