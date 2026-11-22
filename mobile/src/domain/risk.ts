import type { CellRisk, RiskLevel, WeatherCondition } from '../api/types';

export const RISK_LABEL: Record<RiskLevel, string> = { safe: 'Aman', caution: 'Waspada', danger: 'Bahaya' };

export const RISK_ORDER: Record<RiskLevel, number> = { safe: 0, caution: 1, danger: 2 };

export const WEATHER: Record<WeatherCondition, { label: string; icon: string }> = {
  clear: { label: 'Cerah', icon: 'weather-sunny' },
  cloudy: { label: 'Berawan', icon: 'weather-cloudy' },
  rain: { label: 'Hujan', icon: 'weather-rainy' },
  heavy_rain: { label: 'Hujan lebat', icon: 'weather-pouring' },
  storm: { label: 'Badai', icon: 'weather-lightning-rainy' },
};

function factor(cell: CellRisk | null, type: 'rain' | 'verified_report' | 'disaster_signal') {
  return cell?.factors.find((f) => f.type === type);
}

/** Short headline next to the risk badge on `home`. */
export function riskHeadline(cell: CellRisk | null): string {
  if (!cell) return 'Data risiko belum tersedia';
  if (cell.risk_level === 'safe') return 'Aman untuk bepergian';
  if (cell.risk_level === 'danger') return 'Tunda perjalanan dulu';
  const reports = factor(cell, 'verified_report')?.count ?? 0;
  const rain = factor(cell, 'rain')?.value ?? 0;
  if (reports > 0) return 'Hati-hati, ada kejadian';
  if (rain >= 10) return 'Hati-hati, hujan lebat';
  return 'Hati-hati di jalan';
}

/** One plain sentence that turns the risk factors into words (spec §7.1). */
export function riskWhy(cell: CellRisk | null): string {
  if (!cell) return 'Data risiko belum tersedia.';
  const rain = factor(cell, 'rain')?.value ?? 0;
  const reports = factor(cell, 'verified_report')?.count ?? 0;
  const disasters = factor(cell, 'disaster_signal')?.count ?? 0;
  const parts: string[] = [];
  if (rain >= 10) parts.push(`hujan lebat (${Math.round(rain)} mm/jam)`);
  else if (rain >= 2) parts.push(`hujan (${Math.round(rain)} mm/jam)`);
  if (reports > 0) parts.push(`${reports} kejadian yang sudah dipastikan warga`);
  if (disasters > 0) parts.push('sinyal bencana resmi di dekat sini');
  if (cell.risk_level === 'safe') {
    return parts.length === 0
      ? 'Cuaca baik dan tidak ada kejadian yang sudah dipastikan warga di area ini.'
      : `Ada ${parts.join(' dan ')}, tapi belum cukup untuk menaikkan risiko.`;
  }
  const lead = cell.risk_level === 'danger' ? 'Risiko tinggi karena ' : 'Risiko sedang karena ';
  return lead + (parts.length ? parts.join(' dan ') : 'beberapa faktor sekaligus') + '.';
}
