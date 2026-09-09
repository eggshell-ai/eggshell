'use client';

// material-ui
import { useTheme } from '@mui/material/styles';
import { Box, Typography } from '@mui/material';

// project imports
import customization from 'customization';

// ==============================|| LOGO MAIN ||============================== //

export default function LogoMain() {
  const theme = useTheme();

  return (
    <Box sx={{ display: 'flex', alignItems: 'center', gap: 1.25 }}>
      {/* eslint-disable-next-line @next/next/no-img-element */}
      <Box
        component="img"
        src={`/assets/images/${customization.logo}`}
        alt=""
        aria-hidden="true"
        sx={{ display: 'block', width: 32, height: 32 }}
      />
      <Typography
        variant="h5"
        noWrap
        sx={{
          fontWeight: 700,
          letterSpacing: '-0.02em',
          color: theme.vars.palette.text.primary
        }}
      >
        {customization.title}
      </Typography>
    </Box>
  );
}
