'use client';

// material-ui
import Box from '@mui/material/Box';
import Container from '@mui/material/Container';

// ==============================|| SIMPLE LAYOUT ||============================== //

export default function SimpleLayout({ children }) {
  return (
    <Box
      sx={{
        display: 'flex',
        minHeight: '100vh',
        alignItems: 'center',
        justifyContent: 'center',
        bgcolor: 'background.default'
      }}
    >
      <Container maxWidth="sm">{children}</Container>
    </Box>
  );
}
