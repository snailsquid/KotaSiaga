import React from 'react';
import { Pressable, StyleSheet, View } from 'react-native';
import { IconButton, Text, useTheme } from 'react-native-paper';
import { MaterialCommunityIcons } from '@expo/vector-icons';
import type { Report } from '../api/types';
import { CATEGORIES, CONFIRM_TARGET } from '../domain/categories';
import { distanceM } from '../domain/geo';
import { formatDistance } from '../domain/format';
import { placeLabel, reportTitle, useStore } from '../state/AppStore';
import type { AppTheme } from '../theme/theme';

export function useAppTheme() {
  return useTheme<AppTheme>();
}

export function SheetHeader({ title, subtitle, onClose, closeLabel = 'Tutup' }: { title: string; subtitle?: string; onClose?: () => void; closeLabel?: string }) {
  const t = useAppTheme();
  return (
    <View style={styles.header}>
      <View style={{ flex: 1, gap: 4 }}>
        <Text variant="titleLarge" accessibilityRole="header">
          {title}
        </Text>
        {subtitle ? (
          <Text variant="bodyMedium" style={{ color: t.colors.onSurfaceVariant }}>
            {subtitle}
          </Text>
        ) : null}
      </View>
      {onClose ? (
        <IconButton icon="close" mode="contained" containerColor={t.colors.surfaceContainerHighest} iconColor={t.colors.onSurfaceVariant} accessibilityLabel={closeLabel} onPress={onClose} style={{ margin: 0 }} />
      ) : null}
    </View>
  );
}

export function SectionLabel({ children }: { children: React.ReactNode }) {
  const t = useAppTheme();
  return (
    <Text variant="titleSmall" style={{ color: t.colors.onSurfaceVariant, paddingBottom: 4 }} accessibilityRole="header">
      {children}
    </Text>
  );
}

export function Muted({ children, variant = 'bodyMedium' as const, style }: { children: React.ReactNode; variant?: 'bodyMedium' | 'bodySmall' | 'labelMedium'; style?: object }) {
  const t = useAppTheme();
  return (
    <Text variant={variant} style={[{ color: t.colors.onSurfaceVariant }, style]}>
      {children}
    </Text>
  );
}

/** MD3 list item: 72 dp two-line row, whole row is the touch target. */
export function Row({ icon, title, subtitle, trailing, onPress, a11y, bold }: { icon: string; title: string; subtitle?: string; trailing?: React.ReactNode; onPress?: () => void; a11y?: string; bold?: boolean }) {
  const t = useAppTheme();
  return (
    <Pressable
      onPress={onPress}
      accessibilityRole={onPress ? 'button' : undefined}
      accessibilityLabel={a11y}
      android_ripple={{ color: t.colors.surfaceContainerHighest }}
      style={styles.row}
    >
      <MaterialCommunityIcons name={icon as any} size={24} color={t.colors.onSurfaceVariant} />
      <View style={{ flex: 1, minWidth: 0 }}>
        <Text variant="bodyLarge" style={bold ? { fontWeight: '500' } : undefined}>
          {title}
        </Text>
        {subtitle ? (
          <Text variant="bodyMedium" style={{ color: t.colors.onSurfaceVariant }}>
            {subtitle}
          </Text>
        ) : null}
      </View>
      {trailing}
    </Pressable>
  );
}

/** "Perlu dipastikan (1/3)" / "Dipastikan 5 warga". Never "Menunggu" (spec §12b.2). */
export function statusShort(r: Report) {
  const n = r.confirm_count ?? 0;
  if (r.status === 'expired') return 'Kedaluwarsa';
  return r.status === 'verified' ? `Dipastikan ${n} warga` : `Perlu dipastikan (${Math.min(n, CONFIRM_TARGET)}/${CONFIRM_TARGET})`;
}

export function ReportRow({ r }: { r: Report }) {
  const s = useStore();
  const dist = s.me ? formatDistance(distanceM(s.me, r)) : null;
  const mine = s.myReports.has(r.id);
  const sub = [placeLabel(r), dist, mine ? 'Laporanmu' : statusShort(r)].filter(Boolean).join(' · ');
  return (
    <Row
      icon={CATEGORIES[r.category].icon}
      title={reportTitle(r)}
      subtitle={sub}
      onPress={() => s.openReport(r.id)}
      a11y={`${CATEGORIES[r.category].label}: ${reportTitle(r)}. ${sub}`}
    />
  );
}

export function InlineError({ message, onRetry }: { message: string; onRetry?: () => void }) {
  const t = useAppTheme();
  return (
    <View style={[styles.err, { backgroundColor: t.colors.errorContainer }]} accessibilityRole="alert" accessibilityLiveRegion="polite">
      <Text variant="bodyMedium" style={{ color: t.colors.onErrorContainer, flex: 1 }}>
        {message}
      </Text>
      {onRetry ? (
        <Text variant="labelLarge" onPress={onRetry} accessibilityRole="button" style={{ color: t.colors.onErrorContainer, padding: 8 }}>
          Coba lagi
        </Text>
      ) : null}
    </View>
  );
}

/** Placeholder rows while loading: loading never blocks the map (spec §1.5). */
export function Placeholder({ rows = 3 }: { rows?: number }) {
  const t = useAppTheme();
  return (
    <View accessibilityLabel="Memuat" style={{ gap: 12 }}>
      {Array.from({ length: rows }).map((_, k) => (
        <View key={k} style={{ height: 56, borderRadius: 12, backgroundColor: t.colors.surfaceContainer }} />
      ))}
    </View>
  );
}

export const styles = StyleSheet.create({
  header: { flexDirection: 'row', alignItems: 'flex-start', gap: 8 },
  row: { flexDirection: 'row', alignItems: 'center', gap: 16, minHeight: 72, paddingVertical: 8 },
  card: { borderRadius: 12, padding: 16, gap: 12 },
  outlined: { borderRadius: 12, borderWidth: 1, paddingVertical: 12, paddingHorizontal: 16, gap: 8 },
  buttonsRow: { flexDirection: 'row', gap: 8, flexWrap: 'wrap' },
  err: { flexDirection: 'row', alignItems: 'center', gap: 8, borderRadius: 12, paddingLeft: 16, paddingRight: 4, paddingVertical: 4 },
});
