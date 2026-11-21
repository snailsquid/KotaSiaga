// Mock place data for Bandung. Test data only, approximate coordinates.
// No contract endpoint exists yet for place search, reverse geocoding, tourism points or
// blood-donation points on the mobile side (see README "Spec gaps"). Everything that reads
// this file goes through ApiService so it can be swapped for real endpoints later.

import type { Point } from '../types';
import { distToSegmentM, distanceM } from '../../domain/geo';

export interface Street {
  name: string;
  path: [number, number][]; // [lat, lon]
}

export const STREETS: Street[] = [
  { name: 'Jl. Pasteur', path: [[-6.8932, 107.5840], [-6.8938, 107.5950], [-6.8942, 107.6040]] },
  { name: 'Jl. Ir. H. Juanda', path: [[-6.8600, 107.6175], [-6.8800, 107.6145], [-6.8950, 107.6130], [-6.9040, 107.6120]] },
  { name: 'Jl. Cihampelas', path: [[-6.8790, 107.6035], [-6.8900, 107.6040], [-6.9020, 107.6050]] },
  { name: 'Jl. Tamansari', path: [[-6.8840, 107.6080], [-6.8960, 107.6085], [-6.9110, 107.6080]] },
  { name: 'Jl. Merdeka', path: [[-6.9040, 107.6100], [-6.9110, 107.6105], [-6.9180, 107.6110]] },
  { name: 'Jl. Asia Afrika', path: [[-6.9215, 107.5980], [-6.9212, 107.6080], [-6.9205, 107.6250]] },
  { name: 'Jl. Dr. Setiabudi', path: [[-6.8540, 107.5960], [-6.8700, 107.5970], [-6.8850, 107.5990]] },
  { name: 'Jl. Sukajadi', path: [[-6.8700, 107.5935], [-6.8800, 107.5950], [-6.8920, 107.5970]] },
  { name: 'Jl. RE. Martadinata', path: [[-6.9060, 107.6110], [-6.9055, 107.6200], [-6.9050, 107.6300]] },
  { name: 'Jl. Soekarno-Hatta', path: [[-6.9400, 107.5700], [-6.9420, 107.6200], [-6.9440, 107.6800]] },
  { name: 'Jl. Buah Batu', path: [[-6.9250, 107.6270], [-6.9400, 107.6320], [-6.9550, 107.6350]] },
  { name: 'Jl. Ahmad Yani', path: [[-6.9130, 107.6250], [-6.9080, 107.6450], [-6.9050, 107.6600]] },
  { name: 'Jl. Dipatiukur', path: [[-6.8870, 107.6165], [-6.8930, 107.6170], [-6.9000, 107.6175]] },
  { name: 'Jl. Surapati', path: [[-6.8990, 107.6150], [-6.8990, 107.6280], [-6.8985, 107.6400]] },
  { name: 'Jl. Pajajaran', path: [[-6.9060, 107.5850], [-6.9065, 107.5950], [-6.9070, 107.6050]] },
  { name: 'Jl. Pasirkaliki', path: [[-6.8950, 107.5990], [-6.9050, 107.5995], [-6.9150, 107.6000]] },
];

const DISTRICTS: { name: string; lat: number; lon: number }[] = [
  { name: 'Coblong', lat: -6.888, lon: 107.615 },
  { name: 'Sukajadi', lat: -6.884, lon: 107.594 },
  { name: 'Cicendo', lat: -6.905, lon: 107.592 },
  { name: 'Bandung Wetan', lat: -6.904, lon: 107.619 },
  { name: 'Sumur Bandung', lat: -6.917, lon: 107.611 },
  { name: 'Cidadap', lat: -6.863, lon: 107.604 },
  { name: 'Lengkong', lat: -6.930, lon: 107.625 },
  { name: 'Regol', lat: -6.935, lon: 107.605 },
  { name: 'Batununggal', lat: -6.930, lon: 107.640 },
  { name: 'Cibeunying Kaler', lat: -6.890, lon: 107.636 },
  { name: 'Andir', lat: -6.913, lon: 107.578 },
  { name: 'Astanaanyar', lat: -6.925, lon: 107.598 },
  { name: 'Buahbatu', lat: -6.948, lon: 107.640 },
  { name: 'Antapani', lat: -6.912, lon: 107.661 },
  { name: 'Sukasari', lat: -6.868, lon: 107.585 },
];

export type PlaceKind = 'place' | 'tourism' | 'donor';

export interface PlaceEntry {
  id: string;
  kind: PlaceKind;
  name: string;
  address: string;
  lat: number;
  lon: number;
  /** Tourism category (JelajahLokal feed shape: id, name, category, lat, lon). */
  category?: string;
  /** Blood-donation point fields (TitikTujuan: opens_at, closes_at). Free slots are mock-only. */
  opens_at?: string;
  closes_at?: string;
  free_slots?: number;
}

export const PLACES: PlaceEntry[] = [
  { id: 'pl_itb', kind: 'place', name: 'Kampus ITB Ganesha', address: 'Jl. Ganesha No. 10', lat: -6.8915, lon: 107.6107 },
  { id: 'pl_bip', kind: 'place', name: 'Bandung Indah Plaza', address: 'Jl. Merdeka No. 56', lat: -6.9083, lon: 107.6106 },
  { id: 'pl_stasiun', kind: 'place', name: 'Stasiun Bandung', address: 'Jl. Stasiun Barat', lat: -6.9142, lon: 107.6025 },
  { id: 'pl_alun', kind: 'place', name: 'Alun-alun Bandung', address: 'Jl. Asia Afrika', lat: -6.9217, lon: 107.6071 },
  { id: 'pl_rshs', kind: 'place', name: 'RS Hasan Sadikin', address: 'Jl. Pasteur No. 38', lat: -6.8960, lon: 107.5985 },
  { id: 'pl_unpad', kind: 'place', name: 'Unpad Dipatiukur', address: 'Jl. Dipatiukur No. 35', lat: -6.8935, lon: 107.6170 },
  // Tourism (mock of JelajahLokal /feed/destinations)
  { id: 'dst_sate', kind: 'tourism', name: 'Gedung Sate', address: 'Jl. Diponegoro No. 22', lat: -6.9025, lon: 107.6188, category: 'Sejarah' },
  { id: 'dst_braga', kind: 'tourism', name: 'Jalan Braga', address: 'Jl. Braga', lat: -6.9175, lon: 107.6095, category: 'Kuliner dan belanja' },
  { id: 'dst_tahura', kind: 'tourism', name: 'Tahura Djuanda', address: 'Jl. Ir. H. Juanda No. 99', lat: -6.8566, lon: 107.6326, category: 'Alam' },
  { id: 'dst_udjo', kind: 'tourism', name: 'Saung Angklung Udjo', address: 'Jl. Padasuka No. 118', lat: -6.8980, lon: 107.6550, category: 'Budaya' },
  { id: 'dst_zoo', kind: 'tourism', name: 'Bandung Zoo', address: 'Jl. Tamansari No. 6', lat: -6.8897, lon: 107.6074, category: 'Keluarga' },
  { id: 'dst_babakan', kind: 'tourism', name: 'Babakan Siliwangi', address: 'Jl. Siliwangi', lat: -6.8850, lon: 107.6070, category: 'Alam' },
  // Blood donation (mock of DonorKita events / UDD locations)
  { id: 'dnr_pmi', kind: 'donor', name: 'PMI Kota Bandung', address: 'Jl. Aceh No. 79', lat: -6.9103, lon: 107.6143, opens_at: '08:00', closes_at: '20:00', free_slots: 14 },
  { id: 'dnr_pusdai', kind: 'donor', name: 'Donor darah Pusdai', address: 'Jl. Diponegoro No. 63', lat: -6.9006, lon: 107.6267, opens_at: '09:00', closes_at: '14:00', free_slots: 6 },
  { id: 'dnr_itb', kind: 'donor', name: 'Donor darah Salman ITB', address: 'Jl. Ganesha No. 7', lat: -6.8935, lon: 107.6115, opens_at: '10:00', closes_at: '15:00', free_slots: 9 },
];

/** Nearest street name within 250 m, else null. */
export function nearestStreet(p: Point): string | null {
  let best: string | null = null;
  let bestD = 250;
  for (const s of STREETS) {
    for (let i = 0; i < s.path.length - 1; i++) {
      const a = { lat: s.path[i][0], lon: s.path[i][1] };
      const b = { lat: s.path[i + 1][0], lon: s.path[i + 1][1] };
      const d = distToSegmentM(p, a, b);
      if (d < bestD) {
        bestD = d;
        best = s.name;
      }
    }
  }
  return best;
}

export function nearestDistrict(p: Point): string {
  let best = DISTRICTS[0];
  let bestD = Infinity;
  for (const d of DISTRICTS) {
    const dist = distanceM(p, d);
    if (dist < bestD) {
      bestD = dist;
      best = d;
    }
  }
  return best.name;
}
