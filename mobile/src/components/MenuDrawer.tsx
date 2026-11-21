import React, { useEffect, useRef } from 'react';
import { AccessibilityInfo, Animated, Pressable, ScrollView, StyleSheet, View, useWindowDimensions } from 'react-native';
import { Divider, Drawer, Text } from 'react-native-paper';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { ApiService } from '../api/ApiService';
import type { MockScenario } from '../api/types';
import { useStore } from '../state/AppStore';
import { theme } from '../theme/theme';

const SCENARIOS: { name: MockScenario | 'offline'; label: string }[] = [
  { name: 'rain_storm', label: 'Hujan badai di dekatku' },
  { name: 'flood_verified', label: 'Banjir terverifikasi' },
  { name: 'road_closure', label: 'Jalan ditutup' },
  { name: 'event_crowd', label: 'Keramaian' },
  { name: 'disaster_signal', label: 'Sinyal bencana' },
  { name: 'upstream_down', label: 'Sumber data mati (stale)' },
  { name: 'offline', label: 'Tidak ada koneksi (aktif/nonaktif)' },
  { name: 'recover', label: 'Pulihkan' },
  { name: 'reset', label: 'Reset data demo' },
];

/** Side drawer from ≡ (spec §3.4). Overlays the map, closes on tap outside. */
export function MenuDrawer({ open, onClose }: { open: boolean; onClose: () => void }) {
  const s = useStore();
  const insets = useSafeAreaInsets();
  const { width } = useWindowDimensions();
  const w = Math.min(320, width - 56);
  const x = useRef(new Animated.Value(-w)).current;

  useEffect(() => {
    AccessibilityInfo.isReduceMotionEnabled().then((reduce) => {
      Animated.timing(x, { toValue: open ? 0 : -w, duration: reduce ? 0 : 220, useNativeDriver: true }).start();
    });
  }, [open, w, x]);

  if (!open) return null;

  const pick = (fn: () => void) => () => {
    onClose();
    fn();
  };

  return (
    <View style={StyleSheet.absoluteFill}>
      <Pressable style={[StyleSheet.absoluteFill, { backgroundColor: theme.colors.backdrop }]} onPress={onClose} accessibilityLabel="Tutup menu" />
      <Animated.View style={[styles.drawer, { width: w, paddingTop: insets.top + 12, transform: [{ translateX: x }] }]} accessibilityViewIsModal>
        <ScrollView>
          <Text variant="titleSmall" style={styles.title}>
            KotaSiaga
          </Text>
          <Drawer.Item icon="bell-outline" label="Notifikasi" onPress={pick(() => s.need({ type: 'alerts' }, 'melihat notifikasi'))} />
          <Drawer.Item icon="map-marker-radius-outline" label="Area pantauan" onPress={pick(() => s.need({ type: 'subscriptions' }, 'memantau area'))} />
          <Drawer.Item icon="format-list-bulleted" label="Laporan di sekitar" onPress={pick(() => s.go({ view: 'list' }))} />
          <Drawer.Item icon="information-outline" label="Tentang aplikasi" onPress={pick(() => s.go({ view: 'about' }))} />
          {ApiService.isMock ? (
            <>
              <Divider style={{ marginVertical: 8 }} />
              <Text variant="titleSmall" style={styles.title}>
                Mode demo (MOCK)
              </Text>
              {SCENARIOS.map((sc) => (
                <Drawer.Item
                  key={sc.name}
                  icon="play-circle-outline"
                  label={sc.label}
                  onPress={pick(() => {
                    ApiService.mockScenario(sc.name, s.me ?? undefined)
                      .then(() => {
                        s.showToast(`Skenario: ${sc.label}`);
                        s.refreshAll();
                      })
                      .catch(() => s.showToast('Skenario gagal dijalankan'));
                  })}
                />
              ))}
            </>
          ) : null}
        </ScrollView>
      </Animated.View>
    </View>
  );
}

const styles = StyleSheet.create({
  drawer: {
    position: 'absolute',
    top: 0,
    bottom: 0,
    left: 0,
    backgroundColor: theme.colors.surfaceContainerLow,
    borderTopRightRadius: 16,
    borderBottomRightRadius: 16,
    paddingHorizontal: 12,
  },
  title: { paddingHorizontal: 16, paddingVertical: 16, color: theme.colors.onSurfaceVariant },
});
