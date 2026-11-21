import React, { useRef } from 'react';
import { Linking, Pressable, StyleSheet, View, useWindowDimensions } from 'react-native';
import { Checkbox, Divider, FAB, Text } from 'react-native-paper';
import Animated, { useAnimatedStyle, type SharedValue } from 'react-native-reanimated';
import { useStore } from '../state/AppStore';
import { riskColors, theme } from '../theme/theme';
import type { Layers } from '../state/types';

const c = theme.colors;

/**
 * Map buttons on the right edge + the extended "Lapor" FAB. They ride on top of the sheet
 * (spec §2: slide up with the sheet, hidden at full height; "Lapor" only at peek).
 */
export function MapControls({ sheetTop, visible, showLapor, layersOpen, onToggleLayers }: { sheetTop: SharedValue<number>; visible: boolean; showLapor: boolean; layersOpen: boolean; onToggleLayers: () => void }) {
  const s = useStore();
  const { height } = useWindowDimensions();
  const follow = useRef(false);
  const style = useAnimatedStyle(() => ({ bottom: Math.max(0, height - sheetTop.value) + 16 }));

  if (!visible) return null;

  const locate = () => {
    if (s.location === 'denied') {
      s.showToast('Izinkan lokasi di Pengaturan', { label: 'Buka Pengaturan', onPress: () => Linking.openSettings() });
      return;
    }
    if (s.location === 'unknown') {
      s.go({ view: 'location_intro' });
      return;
    }
    if (!s.me) return;
    s.mapApi.current?.flyTo(s.me, true);
    // Second tap: follow mode. Heading rotation is not implemented yet (see README).
    if (follow.current) s.showToast('Mengikuti lokasimu');
    follow.current = !follow.current;
  };

  return (
    <Animated.View style={[styles.wrap, style]} pointerEvents="box-none">
      <FAB
        icon="layers"
        size="small"
        variant="surface"
        onPress={onToggleLayers}
        accessibilityLabel="Lapisan peta"
        accessibilityState={{ expanded: layersOpen }}
        style={[styles.small, { backgroundColor: c.surfaceContainerHigh }]}
        color={c.onSurfaceVariant}
      />
      <FAB icon="crosshairs-gps" variant="surface" onPress={locate} accessibilityLabel="Lokasi saya" style={{ backgroundColor: c.surfaceContainerHigh }} color={c.primary} />
      {showLapor ? (
        <FAB
          icon="map-marker-plus-outline"
          label="Lapor"
          variant="primary"
          onPress={() => s.go({ view: 'quick' })}
          accessibilityLabel="Lapor kejadian"
          style={{ backgroundColor: c.primaryContainer }}
          color={c.onPrimaryContainer}
        />
      ) : null}
    </Animated.View>
  );
}

const LAYER_ROWS: { key: keyof Layers; label: string; unavailable?: boolean }[] = [
  { key: 'risk', label: 'Area risiko' },
  { key: 'reports', label: 'Laporan warga' },
  { key: 'disasters', label: 'Sinyal bencana' },
  { key: 'tourism', label: 'Destinasi wisata' },
  { key: 'donor', label: 'Lokasi donor darah' },
];

/** "Lapisan" card over the map, not the main sheet (spec §4.4). Closes on tap outside. */
export function LayersMenu({ sheetTop, onClose }: { sheetTop: SharedValue<number>; onClose: () => void }) {
  const s = useStore();
  const { height } = useWindowDimensions();
  const style = useAnimatedStyle(() => ({ bottom: Math.max(0, height - sheetTop.value) + 16 + 60 }));
  return (
    <>
      <Pressable style={StyleSheet.absoluteFill} onPress={onClose} accessibilityLabel="Tutup lapisan peta" />
      <Animated.View style={[styles.layers, style]} accessibilityViewIsModal>
        {LAYER_ROWS.map((row) => (
          <Checkbox.Item
            key={row.key}
            label={row.unavailable ? `${row.label} · Data tidak tersedia` : row.label}
            status={s.layers[row.key] ? 'checked' : 'unchecked'}
            disabled={row.unavailable}
            onPress={() => s.setLayers({ ...s.layers, [row.key]: !s.layers[row.key] })}
            position="leading"
            labelVariant="labelLarge"
            style={styles.layerRow}
            labelStyle={{ textAlign: 'left' }}
          />
        ))}
        <Divider style={{ marginVertical: 8 }} />
        <View style={styles.legend} accessible accessibilityLabel="Keterangan: lingkaran terisi berarti sudah dipastikan warga. Lingkaran putus-putus berarti perlu dipastikan. Oranye berarti Waspada. Merah berarti Bahaya.">
          <LegendRow swatch={<View style={[styles.dot, { backgroundColor: c.inverseSurface, borderColor: '#FFF', borderStyle: 'solid' }]} />} text="Sudah dipastikan warga" />
          <LegendRow swatch={<View style={[styles.dot, { backgroundColor: '#FFF', borderColor: c.onSurfaceVariant, borderStyle: 'dashed' }]} />} text="Perlu dipastikan" />
          <LegendRow swatch={<View style={[styles.dot, { backgroundColor: c.primaryContainer, borderColor: c.primary, borderStyle: 'dashed' }]} />} text="Laporanmu" />
          <LegendRow swatch={<View style={[styles.line, { backgroundColor: riskColors.caution.line }]} />} text="Waspada (jalan atau area)" />
          <LegendRow swatch={<View style={[styles.line, { backgroundColor: riskColors.danger.line }]} />} text="Bahaya (jalan atau area)" />
        </View>
      </Animated.View>
    </>
  );
}

function LegendRow({ swatch, text }: { swatch: React.ReactNode; text: string }) {
  return (
    <View style={{ flexDirection: 'row', alignItems: 'center', gap: 10 }}>
      <View style={{ width: 22, alignItems: 'center' }}>{swatch}</View>
      <Text variant="bodySmall" style={{ color: c.onSurfaceVariant }}>
        {text}
      </Text>
    </View>
  );
}

const styles = StyleSheet.create({
  wrap: { position: 'absolute', right: 16, alignItems: 'flex-end', gap: 16 },
  small: { marginRight: 8 },
  layers: {
    position: 'absolute',
    right: 72,
    width: 248,
    backgroundColor: c.surfaceContainer,
    borderRadius: 4,
    paddingVertical: 8,
    elevation: 3,
    shadowColor: '#000',
    shadowOpacity: 0.2,
    shadowRadius: 4,
    shadowOffset: { width: 0, height: 2 },
  },
  layerRow: { minHeight: 48, paddingVertical: 0, paddingHorizontal: 8 },
  legend: { gap: 8, paddingHorizontal: 12, paddingBottom: 8 },
  dot: { width: 18, height: 18, borderRadius: 9, borderWidth: 2 },
  line: { width: 22, height: 5, borderRadius: 3 },
});
