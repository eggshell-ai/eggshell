'use client';

import { useEffect, useRef, useState } from 'react';
import { useRouter } from 'next/navigation';

// material-ui
import ButtonBase from '@mui/material/ButtonBase';
import CardContent from '@mui/material/CardContent';
import ClickAwayListener from '@mui/material/ClickAwayListener';
import Paper from '@mui/material/Paper';
import Popper from '@mui/material/Popper';
import Stack from '@mui/material/Stack';
import Tooltip from '@mui/material/Tooltip';
import Typography from '@mui/material/Typography';
import Box from '@mui/material/Box';

// project imports
import ProfileTab from './ProfileTab';
import Avatar from 'components/@extended/Avatar';
import MainCard from 'components/MainCard';
import Transitions from 'components/@extended/Transitions';
import IconButton from 'components/@extended/IconButton';
import authService from 'services/authService';
import profileService from 'api/profileService';

// assets
import LogoutOutlined from '@ant-design/icons/LogoutOutlined';

// ==============================|| HEADER CONTENT - PROFILE ||============================== //

export default function Profile() {
  const router = useRouter();

  const anchorRef = useRef(null);
  const [open, setOpen] = useState(false);
  const [user, setUser] = useState(null);
  const [avatarUrl, setAvatarUrl] = useState(null);

  useEffect(() => {
    const currentUser = authService.getUser();
    setUser(currentUser);

    // Fetch user profile to check for avatar
    profileService
      .getMyProfile()
      .then((profile) => {
        if (profile?.avatar) {
          const baseUrl = process.env.NEXT_PUBLIC_API_URL?.replace(/\/api\/?$/, '') || 'http://localhost:8000';
          const fullAvatarUrl = profile.avatar.startsWith('http')
            ? profile.avatar
            : `${baseUrl}/${profile.avatar.replace(/^\//, '')}`;
          setAvatarUrl(fullAvatarUrl);
        }
      })
      .catch(() => {
        // Silently catch if not yet authenticated or profile endpoint unavailable
      });
  }, []);

  const handleToggle = () => {
    setOpen((prevOpen) => !prevOpen);
  };

  const handleClose = (event) => {
    if (anchorRef.current && anchorRef.current.contains(event.target)) {
      return;
    }
    setOpen(false);
  };

  const handleLogout = () => {
    authService.logout();
    router.push('/login');
  };


  const displayName = user
    ? (user.first_name ? `${user.first_name} ${user.last_name || ''}`.trim() : (typeof user === 'string' ? user : user.email || 'User'))
    : 'User';

  const userInitial = (displayName || 'U').charAt(0).toUpperCase();

  const userRole = (user?.role || 'User').replace(/^ROLE_/, '');

  return (
    <Box sx={{ flexShrink: 0, ml: 'auto' }}>
      <Tooltip title="Profile" disableInteractive>
        <ButtonBase
          sx={(theme) => ({
            p: 0.25,
            borderRadius: 1,
            '&:focus-visible': { outline: `2px solid ${theme.vars.palette.secondary.dark}`, outlineOffset: 2 }
          })}
          aria-label="open profile"
          ref={anchorRef}
          aria-controls={open ? 'profile-grow' : undefined}
          aria-haspopup="true"
          onClick={handleToggle}
        >
          {avatarUrl ? (
            <Avatar alt={displayName} src={avatarUrl} size="sm" sx={{ '&:hover': { outline: '1px solid', outlineColor: 'primary.main' } }} />
          ) : (
            <Avatar size="sm" color="primary" sx={{ '&:hover': { outline: '1px solid', outlineColor: 'primary.main' }, fontWeight: 600 }}>
              {userInitial}
            </Avatar>
          )}
        </ButtonBase>
      </Tooltip>
      <Popper
        placement="bottom-end"
        open={open}
        anchorEl={anchorRef.current}
        role={undefined}
        transition
        disablePortal
        popperOptions={{
          modifiers: [
            {
              name: 'offset',
              options: {
                offset: [0, 9]
              }
            }
          ]
        }}
      >
        {({ TransitionProps }) => (
          <Transitions type="grow" position="top-right" in={open} {...TransitionProps}>
            <Paper sx={(theme) => ({ boxShadow: theme.vars?.customShadows?.z1 || theme.shadows[2], width: 290, minWidth: 240, maxWidth: { xs: 250, md: 290 } })}>
              <ClickAwayListener onClickAway={handleClose}>
                <MainCard elevation={0} border={false} content={false}>
                  <CardContent sx={{ px: 2.5, pt: 3 }}>
                    <Stack direction="row" sx={{ justifyContent: 'space-between', alignItems: 'center' }}>
                      <Stack direction="row" sx={{ gap: 1.25, alignItems: 'center' }}>
                        {avatarUrl ? (
                          <Avatar alt={displayName} src={avatarUrl} sx={{ width: 32, height: 32 }} />
                        ) : (
                          <Avatar sx={{ width: 32, height: 32, fontWeight: 600 }} color="primary">
                            {userInitial}
                          </Avatar>
                        )}
                        <Stack>
                          <Typography variant="h6">{displayName}</Typography>
                          <Typography variant="body2" sx={{ color: 'text.secondary' }}>
                            {userRole}
                          </Typography>
                        </Stack>
                      </Stack>
                      <Tooltip title="Logout">
                        <IconButton size="large" sx={{ color: 'text.primary' }} onClick={handleLogout}>
                          <LogoutOutlined />
                        </IconButton>
                      </Tooltip>
                    </Stack>
                  </CardContent>

                  <Box sx={{ p: 1 }}>
                    <ProfileTab handleLogout={handleLogout} onClose={() => setOpen(false)} />
                  </Box>
                </MainCard>
              </ClickAwayListener>
            </Paper>
          </Transitions>
        )}
      </Popper>
    </Box>
  );
}
