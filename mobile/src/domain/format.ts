// Indonesian formatting helpers. Units per AGENTS.md §3: meters, seconds, °C.

export function formatDistance(m: number): string {
  if (m < 1000) return `${Math.max(10, Math.round(m / 10) * 10)} m`;
  return `${(m / 1000).toFixed(1).replace('.', ',')} km`;
}

/** Spoken form for screen readers: "300 meter", "1,2 kilometer". */
export function spokenDistance(m: number): string {
  if (m < 1000) return `${Math.round(m / 10) * 10} meter`;
  return `${(m / 1000).toFixed(1).replace('.', ',')} kilometer`;
}

export function formatDuration(s: number): string {
  const min = Math.max(1, Math.round(s / 60));
  if (min < 60) return `${min} mnt`;
  return `${Math.floor(min / 60)} j ${min % 60} mnt`;
}

export function ago(iso: string, now = Date.now()): string {
  const s = Math.max(0, (now - Date.parse(iso)) / 1000);
  if (s < 60) return 'baru saja';
  const m = Math.floor(s / 60);
  if (m < 60) return `${m} menit lalu`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h} jam lalu`;
  return `${Math.floor(h / 24)} hari lalu`;
}

export function agoShort(iso: string, now = Date.now()): string {
  const m = Math.floor(Math.max(0, now - Date.parse(iso)) / 60000);
  if (m < 1) return 'baru';
  if (m < 60) return `${m} mnt`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h} j`;
  return `${Math.floor(h / 24)} hari`;
}

export function expiresIn(iso: string, now = Date.now()): string {
  const s = (Date.parse(iso) - now) / 1000;
  if (s <= 0) return 'sudah berakhir';
  const m = Math.round(s / 60);
  if (m < 60) return `dalam ±${Math.max(1, m)} menit`;
  const h = Math.round(m / 60);
  if (h < 24) return `dalam ±${h} jam`;
  return `dalam ±${Math.round(h / 24)} hari`;
}

const pad = (n: number) => (n < 10 ? '0' : '') + n;

/** "10:15:03" in the phone's local time. */
export function clock(iso: string): string {
  const d = new Date(iso);
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

export function hourLabel(iso: string): string {
  return `${pad(new Date(iso).getHours())}.00`;
}
