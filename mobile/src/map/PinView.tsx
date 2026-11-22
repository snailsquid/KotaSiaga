import React, { useEffect, useRef, useState } from 'react';
import { AccessibilityInfo, Animated, StyleSheet, View } from 'react-native';
import { Marker } from 'react-native-maps';
import { MaterialCommunityIcons } from '@expo/vector-icons';
import { Text } from 'react-native-paper';
import { theme } from '../theme/theme';

const c = theme.colors;

export type PinVariant = 'waiting' | 'verified' | 'mine';

/**
 * Report pin. Neutral, never risk-colored (spec §12.5): filled = confirmed, dashed = waiting,
 * dashed blue = mine. New pins pop in unless the system asks for reduced motion.
 */
export function ReportPin(props: {
  id: string;
  lat: number;
  lon: number;
  icon: string;
  variant: PinVariant;
  selected: boolean;
  label: string;
  isNew: boolean;
  onPress: () => void;
}) {
  const { lat, lon, icon, variant, selected, label, isNew, onPress } = props;
  const [track, setTrack] = useState(true);
  const scale = useRef(new Animated.Value(isNew ? 0.4 : 1)).current;

  useEffect(() => {
    let cancelled = false;
    AccessibilityInfo.isReduceMotionEnabled().then((reduce) => {
      if (cancelled) return;
      if (isNew && !reduce) Animated.spring(scale, { toValue: 1, useNativeDriver: true, friction: 5 }).start();
      else scale.setValue(1);
    });
    const t = setTimeout(() => setTrack(false), 700);
    return () => {
      cancelled = true;
      clearTimeout(t);
    };
  }, [isNew, scale]);

  const size = selected ? 48 : 40;
  const look =
    variant === 'mine'
      ? { backgroundColor: c.primaryContainer, borderColor: c.primary, borderStyle: 'dashed' as const, color: c.onPrimaryContainer }
      : variant === 'verified'
        ? { backgroundColor: c.inverseSurface, borderColor: '#FFFFFF', borderStyle: 'solid' as const, color: c.inverseOnSurface }
        : { backgroundColor: '#FFFFFF', borderColor: c.onSurfaceVariant, borderStyle: 'dashed' as const, color: c.onSurface };

  return (
    <Marker
      coordinate={{ latitude: lat, longitude: lon }}
      onPress={onPress}
      tracksViewChanges={track}
      anchor={{ x: 0.5, y: 0.5 }}
      zIndex={selected ? 30 : 20}
      accessibilityLabel={label}
      title={undefined}
    >
      <Animated.View style={{ transform: [{ scale }], padding: 4 }}>
        <View style={[styles.ring, selected && styles.ringOn, { width: size + 8, height: size + 8, borderRadius: (size + 8) / 2 }]}>
          <View
            style={[
              styles.pin,
              { width: size, height: size, borderRadius: size / 2, backgroundColor: look.backgroundColor, borderColor: look.borderColor, borderStyle: look.borderStyle },
            ]}
          >
            <MaterialCommunityIcons name={icon as any} size={selected ? 24 : 20} color={look.color} />
          </View>
        </View>
      </Animated.View>
    </Marker>
  );
}

export function ClusterBubble({ lat, lon, count, onPress }: { lat: number; lon: number; count: number; onPress: () => void }) {
  const [track, setTrack] = useState(true);
  useEffect(() => {
    const t = setTimeout(() => setTrack(false), 500);
    return () => clearTimeout(t);
  }, [count]);
  const size = count >= 10 ? 48 : 44;
  return (
    <Marker
      coordinate={{ latitude: lat, longitude: lon }}
      onPress={onPress}
      tracksViewChanges={track}
      anchor={{ x: 0.5, y: 0.5 }}
      zIndex={25}
      accessibilityLabel={`${count} laporan di sini. Ketuk untuk memperbesar.`}
    >
      <View style={[styles.cluster, { width: size, height: size, borderRadius: size / 2 }]}>
        <Text variant="labelLarge" style={{ color: c.inverseOnSurface }}>
          {count}
        </Text>
      </View>
    </Marker>
  );
}

export function PointMarker(props: { lat: number; lon: number; icon: string; label: string; onPress: () => void }) {
  const [track, setTrack] = useState(true);
  useEffect(() => {
    const t = setTimeout(() => setTrack(false), 500);
    return () => clearTimeout(t);
  }, []);
  return (
    <Marker
      coordinate={{ latitude: props.lat, longitude: props.lon }}
      onPress={props.onPress}
      tracksViewChanges={track}
      anchor={{ x: 0.5, y: 0.5 }}
      zIndex={15}
      accessibilityLabel={props.label}
    >
      <View style={styles.point}>
        <MaterialCommunityIcons name={props.icon as any} size={16} color={c.onTertiaryContainer} />
      </View>
    </Marker>
  );
}

const styles = StyleSheet.create({
  ring: { alignItems: 'center', justifyContent: 'center' },
  ringOn: { backgroundColor: 'rgba(0,97,164,0.45)' },
  pin: {
    alignItems: 'center',
    justifyContent: 'center',
    borderWidth: 2,
    shadowColor: '#000',
    shadowOpacity: 0.3,
    shadowRadius: 2,
    shadowOffset: { width: 0, height: 1 },
    elevation: 3,
  },
  cluster: {
    backgroundColor: c.inverseSurface,
    borderWidth: 3,
    borderColor: '#FFFFFF',
    alignItems: 'center',
    justifyContent: 'center',
    elevation: 3,
  },
  point: {
    width: 28,
    height: 28,
    borderRadius: 14,
    backgroundColor: c.tertiaryContainer,
    borderWidth: 2,
    borderColor: '#FFFFFF',
    alignItems: 'center',
    justifyContent: 'center',
    elevation: 2,
  },
});
