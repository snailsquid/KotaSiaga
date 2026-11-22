import { MD3LightTheme, type MD3Theme } from 'react-native-paper';

// Material Theme Builder scheme from seed #0061A4 (light). Values match docs/design/prototype.dc.html.
// Paper's MD3 color type predates the surfaceContainer roles, so those are added as extra keys.
const colors = {
  ...MD3LightTheme.colors,
  primary: '#0061A4',
  onPrimary: '#FFFFFF',
  primaryContainer: '#D1E4FF',
  onPrimaryContainer: '#001D36',
  secondary: '#535F70',
  onSecondary: '#FFFFFF',
  secondaryContainer: '#D7E3F7',
  onSecondaryContainer: '#101C2B',
  tertiary: '#6B5778',
  onTertiary: '#FFFFFF',
  tertiaryContainer: '#F2DAFF',
  onTertiaryContainer: '#251431',
  error: '#BA1A1A',
  onError: '#FFFFFF',
  errorContainer: '#FFDAD6',
  onErrorContainer: '#410002',
  background: '#F8F9FF',
  onBackground: '#191C20',
  surface: '#F8F9FF',
  onSurface: '#191C20',
  surfaceVariant: '#DFE2EB',
  onSurfaceVariant: '#43474E',
  outline: '#73777F',
  outlineVariant: '#C3C7CF',
  inverseSurface: '#2E3135',
  inverseOnSurface: '#EFF0F7',
  inversePrimary: '#9ECAFF',
  shadow: '#000000',
  scrim: '#000000',
  surfaceDisabled: 'rgba(25, 28, 32, 0.12)',
  onSurfaceDisabled: 'rgba(25, 28, 32, 0.38)',
  backdrop: 'rgba(0, 0, 0, 0.32)',
  elevation: {
    level0: 'transparent',
    level1: '#F2F3FA',
    level2: '#ECEEF4',
    level3: '#E6E8EE',
    level4: '#E4E6EC',
    level5: '#E1E2E8',
  },
  // MD3 surface container roles (not in Paper's type)
  surfaceContainerLowest: '#FFFFFF',
  surfaceContainerLow: '#F2F3FA',
  surfaceContainer: '#ECEEF4',
  surfaceContainerHigh: '#E6E8EE',
  surfaceContainerHighest: '#E1E2E8',
};

// The only additions to MD3: risk as custom colors. Always shown together with the word.
export const riskColors = {
  safe: { container: '#C4EED0', onContainer: '#00210E', line: '#1E8E3E', zone: 'rgba(30,142,62,0.25)' },
  caution: { container: '#FFDDB3', onContainer: '#2A1800', line: '#F29900', zone: 'rgba(242,153,0,0.30)' },
  danger: { container: colors.errorContainer, onContainer: colors.onErrorContainer, line: colors.error, zone: 'rgba(186,26,26,0.28)' },
} as const;

export type AppTheme = MD3Theme & { colors: typeof colors };

export const theme: AppTheme = {
  ...MD3LightTheme,
  colors,
};

/** MD3 elevation level 3 shadow, used by FABs and the search bar on the map. */
export const elevation3 = {
  shadowColor: '#000',
  shadowOpacity: 0.2,
  shadowRadius: 6,
  shadowOffset: { width: 0, height: 3 },
  elevation: 6,
};

export const elevation1 = {
  shadowColor: '#000',
  shadowOpacity: 0.18,
  shadowRadius: 2,
  shadowOffset: { width: 0, height: 1 },
  elevation: 2,
};
