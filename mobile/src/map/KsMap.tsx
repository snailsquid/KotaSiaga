import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { AccessibilityInfo, StyleSheet, View, useWindowDimensions } from 'react-native';
import MapView, { Circle, Marker, Polygon, Polyline, type Region } from 'react-native-maps';
import { MaterialCommunityIcons } from '@expo/vector-icons';
import { Text } from 'react-native-paper';
import { ApiService } from '../api/ApiService';
import type { Point } from '../api/types';
import { config } from '../config';
import { CATEGORIES } from '../domain/categories';
import { circlePolygon, toLatLng } from '../domain/geo';
import { spokenDistance } from '../domain/format';
import { isActive, matchesFilter, placeLabel, useStore } from '../state/AppStore';
import { riskColors, theme } from '../theme/theme';
import { ClusterBubble, PointMarker, ReportPin } from './PinView';
import { riskZones, roadStretches, zoneLabels } from './riskShapes';
import { distanceM } from '../domain/geo';
import { MOCK_ME } from '../api/mock/MockBackend';

const BANDUNG: Region = { latitude: MOCK_ME.lat, longitude: MOCK_ME.lon, latitudeDelta: 0.03, longitudeDelta: 0.03 };
const CITY_DELTA = 0.09; // zoomed out: hide risk, group pins (spec §4.2)
const STREET_DELTA = 0.035; // zoomed in: labels and single pins
// Soft edge: three stacked translucent rings, no stroke (spec §12b.1)
const FEATHER = [
  { r: 470, a: 0.1 },
  { r: 370, a: 0.1 },
  { r: 270, a: 0.12 },
];

const DISASTER_LABEL: Record<string, string> = {
  flood: 'Banjir',
  earthquake: 'Gempa',
  fire: 'Kebakaran',
  haze: 'Kabut asap',
  wind: 'Angin kencang',
  volcano: 'Gunung api',
};
export { DISASTER_LABEL };

function rgba(hex: string, a: number) {
  const n = parseInt(hex.slice(1), 16);
  return `rgba(${(n >> 16) & 255},${(n >> 8) & 255},${n & 255},${a})`;
}

export function KsMap({ sheetHalfPx }: { sheetHalfPx: number }) {
  const s = useStore();
  const { data, layers, filters, nav } = s;
  const ref = useRef<MapView>(null);
  const [region, setRegion] = useState<Region>(BANDUNG);
  const regionRef = useRef(region);
  regionRef.current = region;
  const lastOverlayPress = useRef(0);
  const lastLive = useRef(0);
  const reduceMotion = useRef(false);
  const seenReports = useRef<Set<string> | null>(null);
  const { height } = useWindowDimensions();
  const view = nav.current.view;

  useEffect(() => {
    AccessibilityInfo.isReduceMotionEnabled().then((v) => (reduceMotion.current = v));
  }, []);

  // ---------------------------------------------------------------- camera API for the rest of the app
  useEffect(() => {
    s.mapApi.current = {
      flyTo: (p: Point, zoomIn = true) => {
        const r = regionRef.current;
        const delta = zoomIn ? Math.min(r.latitudeDelta, 0.014) : r.latitudeDelta;
        // Put the item in the middle of the map area left above a half-height sheet (spec §4.3).
        const shift = (sheetHalfPx / height / 2) * delta;
        ref.current?.animateToRegion(
          { latitude: p.lat - shift, longitude: p.lon, latitudeDelta: delta, longitudeDelta: delta * (r.longitudeDelta / r.latitudeDelta) },
          reduceMotion.current ? 0 : 450,
        );
      },
      fit: (points: Point[]) => {
        ref.current?.fitToCoordinates(points.map(toLatLng), {
          edgePadding: { top: 140, right: 48, bottom: sheetHalfPx + 32, left: 48 },
          animated: !reduceMotion.current,
        });
      },
      center: () => ({ lat: regionRef.current.latitude, lon: regionRef.current.longitude }),
    };
  }, [s.mapApi, sheetHalfPx, height]);

  // ---------------------------------------------------------------- derived layers
  const zoomedOut = region.latitudeDelta > CITY_DELTA;
  const streetLevel = region.latitudeDelta <= STREET_DELTA;

  const zones = useMemo(() => riskZones(data.cells), [data.cells]);
  const stretches = useMemo(() => roadStretches(zones), [zones]);
  const labels = useMemo(() => zoneLabels(zones, data.cells, data.reports, data.disasters), [zones, data.cells, data.reports, data.disasters]);

  const visibleReports = useMemo(() => data.reports.filter((r) => isActive(r) && matchesFilter(r, filters)), [data.reports, filters]);

  // Which pins are new since the last render (for the pop).
  const newIds = useMemo(() => {
    const prev = seenReports.current;
    const next = new Set(visibleReports.map((r) => r.id));
    const fresh = new Set<string>();
    if (prev) next.forEach((id) => !prev.has(id) && fresh.add(id));
    seenReports.current = next;
    return fresh;
  }, [visibleReports]);

  const selectedId = view === 'report' ? (nav.current as any).id : view === 'dup' ? (nav.current as any).dupId : null;

  const clusters = useMemo(() => {
    if (region.latitudeDelta <= STREET_DELTA) return null;
    const cell = region.latitudeDelta / (zoomedOut ? 6 : 10);
    const groups = new Map<string, { lat: number; lon: number; ids: string[] }>();
    for (const r of visibleReports) {
      if (r.id === selectedId) continue;
      const k = `${Math.floor(r.lat / cell)}_${Math.floor(r.lon / cell)}`;
      const g = groups.get(k) ?? { lat: 0, lon: 0, ids: [] };
      g.lat += r.lat;
      g.lon += r.lon;
      g.ids.push(r.id);
      groups.set(k, g);
    }
    return [...groups.values()].map((g) => ({ lat: g.lat / g.ids.length, lon: g.lon / g.ids.length, ids: g.ids }));
  }, [visibleReports, region.latitudeDelta, zoomedOut, selectedId]);

  const markOverlay = () => (lastOverlayPress.current = Date.now());

  const openCell = useCallback(
    (cellId: string, at: Point) => {
      markOverlay();
      s.go({ view: 'cell', cellId, at });
    },
    [s],
  );

  // ---------------------------------------------------------------- route + destination
  const routeView = view === 'route' || view === 'route_detail' ? (nav.current as any) : null;
  const placeView = view === 'place' ? (nav.current as any).place : routeView ? routeView.dest : null;
  const routeResult = routeView ? s.routes.result : null;
  const selectedRoute = routeView ? (routeView.view === 'route_detail' ? routeView.routeId : routeView.selected ?? routeResult?.recommended_id) : null;

  const addMode = view === 'subscriptions' && (nav.current as any).add;
  const pickMode = view === 'pick_location';

  return (
    <MapView
      ref={ref}
      style={StyleSheet.absoluteFill}
      initialRegion={BANDUNG}
      showsUserLocation={s.location === 'granted' && config.useDeviceLocation}
      showsMyLocationButton={false}
      showsCompass={false}
      toolbarEnabled={false}
      showsPointsOfInterests={false}
      rotateEnabled
      onRegionChange={(r) => {
        // Live circle / picker label while panning, throttled so the whole UI does not re-render every frame.
        if (!(addMode || pickMode) || Date.now() - lastLive.current < 120) return;
        lastLive.current = Date.now();
        s.setLiveCenter({ lat: r.latitude, lon: r.longitude });
      }}
      onRegionChangeComplete={(r) => {
        // New data for the area appears after the map stops moving (spec §4.2).
        setRegion(r);
        s.setMapCenter({ lat: r.latitude, lon: r.longitude });
        s.setLiveCenter({ lat: r.latitude, lon: r.longitude });
      }}
      onPress={(e) => {
        if ((e.nativeEvent as any).action === 'marker-press' || Date.now() - lastOverlayPress.current < 400) return;
        if (addMode || pickMode) return;
        // Tap on empty map: clear selection, back to home at peek (spec §4.3).
        if (view !== 'home') s.home();
        else if (nav.current.snap > 0) s.dispatch({ type: 'snap', snap: 0 });
      }}
      onLongPress={(e) => {
        const p = { lat: e.nativeEvent.coordinate.latitude, lon: e.nativeEvent.coordinate.longitude };
        const street = placeLabel(p);
        s.openPlace({ kind: 'pin', name: street, address: `${p.lat.toFixed(5)}, ${p.lon.toFixed(5)}`, lat: p.lat, lon: p.lon });
      }}
    >
      {/* Risk: soft zones + colored road stretches. Only Waspada and Bahaya. Hidden when zoomed out. */}
      {layers.risk && !zoomedOut
        ? zones.map((z) =>
            FEATHER.map((f, k) => (
              <Polygon
                key={`${z.cellId}-${k}`}
                coordinates={circlePolygon(z.center, f.r, 24)}
                fillColor={rgba(riskColors[z.level].line, f.a)}
                strokeWidth={0}
                strokeColor="transparent"
                tappable={k === 0}
                onPress={() => openCell(z.cellId, z.center)}
                zIndex={1}
              />
            )),
          )
        : null}
      {layers.risk && !zoomedOut
        ? stretches.map((st) => (
            <Polyline key={st.key} coordinates={st.coords} strokeColor={riskColors[st.level].line} strokeWidth={6} lineCap="round" zIndex={3} />
          ))
        : null}
      {layers.risk && streetLevel
        ? labels.map((l) => (
            <ZoneChip key={l.key} at={l.at} level={l.level} text={l.text} onPress={() => openCell(l.cellId, l.at)} />
          ))
        : null}

      {/* Official disaster signals: shaded area + pin */}
      {layers.disasters
        ? data.disasters
            .filter((d) => d.status === 'active')
            .map((d) => (
              <React.Fragment key={d.id}>
                <Circle center={toLatLng(d)} radius={700} fillColor="rgba(186,26,26,0.12)" strokeColor="rgba(186,26,26,0.5)" strokeWidth={1} zIndex={2} />
                <Marker
                  coordinate={toLatLng(d)}
                  anchor={{ x: 0.5, y: 0.5 }}
                  onPress={() => {
                    markOverlay();
                    s.go({ view: 'disaster', id: d.id });
                    s.mapApi.current?.flyTo(d, false);
                  }}
                  accessibilityLabel={`Sinyal bencana ${DISASTER_LABEL[d.type] ?? d.type}, aktif`}
                  tracksViewChanges={false}
                  zIndex={18}
                >
                  <View style={styles.disaster}>
                    <MaterialCommunityIcons name="alert-octagon" size={18} color={theme.colors.onError} />
                  </View>
                </Marker>
              </React.Fragment>
            ))
        : null}

      {/* Routes: all candidates, selected on top and thicker */}
      {routeResult
        ? [...routeResult.routes]
            .sort((a) => (a.id === selectedRoute ? 1 : -1))
            .map((r) => (
              <Polyline
                key={r.id}
                coordinates={r.geometry.coordinates.map(([lon, lat]) => ({ latitude: lat, longitude: lon }))}
                strokeColor={r.id === selectedRoute ? theme.colors.primary : '#9CBDE0'}
                strokeWidth={r.id === selectedRoute ? 7 : 5}
                lineCap="round"
                lineJoin="round"
                tappable
                zIndex={r.id === selectedRoute ? 11 : 10}
                onPress={() => {
                  markOverlay();
                  if (routeView?.view === 'route') s.dispatch({ type: 'patch', patch: { selected: r.id } as any });
                }}
              />
            ))
        : null}

      {/* Report pins (or count bubbles when zoomed out) */}
      {layers.reports
        ? clusters
          ? clusters.map((g) =>
              g.ids.length === 1 ? (
                <PinFor key={g.ids[0]} id={g.ids[0]} isNew={newIds.has(g.ids[0])} selected={false} onOverlay={markOverlay} />
              ) : (
                <ClusterBubble
                  key={g.ids.join('-')}
                  lat={g.lat}
                  lon={g.lon}
                  count={g.ids.length}
                  onPress={() => {
                    markOverlay();
                    const r = regionRef.current;
                    ref.current?.animateToRegion(
                      { latitude: g.lat, longitude: g.lon, latitudeDelta: r.latitudeDelta / 3, longitudeDelta: r.longitudeDelta / 3 },
                      reduceMotion.current ? 0 : 400,
                    );
                  }}
                />
              ),
            )
          : visibleReports.map((r) => <PinFor key={r.id} id={r.id} isNew={newIds.has(r.id)} selected={r.id === selectedId} onOverlay={markOverlay} />)
        : null}
      {layers.reports && clusters && selectedId ? <PinFor id={selectedId} isNew={false} selected onOverlay={markOverlay} /> : null}

      {/* Optional layers: tourism destinations and blood-donation points (default off, spec §12.12) */}
      {layers.tourism
        ? ApiService.placesOfKind('tourism').map((p) => (
            <PointMarker
              key={p.id}
              lat={p.lat}
              lon={p.lon}
              icon="camera-outline"
              label={`Destinasi wisata ${p.name}`}
              onPress={() => {
                markOverlay();
                s.openPlace({ ...p, kind: 'tourism' });
              }}
            />
          ))
        : null}
      {layers.donor
        ? ApiService.placesOfKind('donor').map((p) => (
            <PointMarker
              key={p.id}
              lat={p.lat}
              lon={p.lon}
              icon="water"
              label={`Lokasi donor darah ${p.name}`}
              onPress={() => {
                markOverlay();
                s.openPlace({ ...p, kind: 'donor' });
              }}
            />
          ))
        : null}

      {/* Destination marker for place and route */}
      {placeView ? (
        <Marker coordinate={toLatLng(placeView)} anchor={{ x: 0.5, y: 1 }} zIndex={40} tracksViewChanges={false} accessibilityLabel={`Tujuan ${placeView.name}`}>
          <View style={{ alignItems: 'center' }}>
            <MaterialCommunityIcons name="map-marker" size={40} color={theme.colors.error} />
          </View>
        </Marker>
      ) : null}

      {/* "Tambah area": circle at the map center, resizes live */}
      {addMode ? (
        <Circle center={toLatLng(s.liveCenter)} radius={s.subRadius} fillColor="rgba(0,97,164,0.15)" strokeColor={theme.colors.primary} strokeWidth={2} zIndex={50} />
      ) : null}

      {/* Me (mock position). With device location the native blue dot is used instead. */}
      {s.me && !config.useDeviceLocation ? (
        <Marker coordinate={toLatLng(s.me)} anchor={{ x: 0.5, y: 0.5 }} tracksViewChanges={false} zIndex={45} accessibilityLabel="Lokasimu">
          <View style={styles.meRing}>
            <View style={styles.meDot} />
          </View>
        </Marker>
      ) : null}
    </MapView>
  );
}

function ZoneChip({ at, level, text, onPress }: { at: Point; level: 'caution' | 'danger'; text: string; onPress: () => void }) {
  const [track, setTrack] = useState(true);
  useEffect(() => {
    setTrack(true);
    const t = setTimeout(() => setTrack(false), 500);
    return () => clearTimeout(t);
  }, [text]);
  const c = riskColors[level];
  return (
    <Marker coordinate={toLatLng(at)} anchor={{ x: 0.5, y: 1.4 }} onPress={onPress} tracksViewChanges={track} zIndex={12} accessibilityLabel={`Area ${text}`}>
      <View style={[styles.chip, { backgroundColor: c.container }]}>
        <MaterialCommunityIcons name="alert" size={14} color={c.onContainer} />
        <Text variant="labelSmall" style={{ color: c.onContainer }}>
          {text}
        </Text>
      </View>
    </Marker>
  );
}

function PinFor({ id, isNew, selected, onOverlay }: { id: string; isNew: boolean; selected: boolean; onOverlay: () => void }) {
  const s = useStore();
  const r = s.data.reports.find((x) => x.id === id);
  if (!r) return null;
  const mine = s.myReports.has(r.id);
  const verified = r.status === 'verified';
  const variant = mine ? 'mine' : verified ? 'verified' : 'waiting';
  const conf = r.confirm_count ?? 0;
  const status = verified ? `dipastikan ${conf} warga` : `perlu dipastikan, ${Math.min(conf, 3)} dari 3`;
  const dist = s.me ? `, ${spokenDistance(distanceM(s.me, r))}` : '';
  return (
    <ReportPin
      key={`${id}-${variant}-${selected}`}
      id={id}
      lat={r.lat}
      lon={r.lon}
      icon={CATEGORIES[r.category].icon}
      variant={variant}
      selected={selected}
      isNew={isNew}
      label={`Laporan ${CATEGORIES[r.category].label.toLowerCase()}, ${status}${dist}`}
      onPress={() => {
        onOverlay();
        s.openReport(r.id);
      }}
    />
  );
}

const styles = StyleSheet.create({
  chip: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 3,
    paddingVertical: 2,
    paddingLeft: 4,
    paddingRight: 6,
    borderRadius: 8,
    elevation: 2,
  },
  disaster: {
    width: 32,
    height: 32,
    borderRadius: 16,
    backgroundColor: theme.colors.error,
    borderWidth: 2,
    borderColor: '#FFFFFF',
    alignItems: 'center',
    justifyContent: 'center',
    elevation: 3,
  },
  meRing: { width: 48, height: 48, borderRadius: 24, backgroundColor: 'rgba(66,133,244,0.18)', alignItems: 'center', justifyContent: 'center' },
  meDot: { width: 18, height: 18, borderRadius: 9, backgroundColor: '#4285F4', borderWidth: 3, borderColor: '#FFFFFF' },
});

