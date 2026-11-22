import React from 'react';
import { StyleSheet, View } from 'react-native';
import { Text } from 'react-native-paper';
import type { RiskLevel } from '../api/types';
import { RISK_LABEL } from '../domain/risk';
import { riskColors } from '../theme/theme';

/** Risk is never color alone: the word is always inside (spec §4.1, §11). */
export function RiskBadge({ level, small }: { level: RiskLevel; small?: boolean }) {
  const c = riskColors[level];
  return (
    <View
      style={[styles.badge, small && styles.small, { backgroundColor: c.container }]}
      accessible
      accessibilityLabel={`Tingkat risiko ${RISK_LABEL[level]}`}
    >
      <Text variant="labelMedium" style={{ color: c.onContainer }}>
        {RISK_LABEL[level]}
      </Text>
    </View>
  );
}

const styles = StyleSheet.create({
  badge: { minHeight: 28, paddingHorizontal: 10, borderRadius: 8, justifyContent: 'center', alignSelf: 'flex-start' },
  small: { minHeight: 24, paddingHorizontal: 8 },
});
