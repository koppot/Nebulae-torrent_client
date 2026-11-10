export function formatBytes(bytes: number, decimals: number = 2): string {
  if (bytes === 0) return (0).toFixed(decimals) + ' B';
  const k = 1024;
  const dm = decimals < 0 ? 0 : decimals;
  const sizes = ['B', 'KiB', 'MiB', 'GiB', 'TiB', 'PiB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  const val = bytes / Math.pow(k, i);
  return `${val.toFixed(dm)} ${sizes[i]}`;
}

export function formatSpeed(bytesPerSec: number): string {
  if (bytesPerSec === 0) return '0.00 B/s';
  const k = 1024;
  const sizes = ['B/s', 'KB/s', 'MB/s', 'GB/s', 'TB/s'];
  const i = Math.min(Math.floor(Math.log(bytesPerSec) / Math.log(k)), sizes.length - 1);
  const val = bytesPerSec / Math.pow(k, i);
  return `${val.toFixed(2)} ${sizes[i]}`;
}

export function formatEta(seconds?: number): string {
  if (seconds === undefined || seconds === null || seconds <= 0 || !isFinite(seconds)) {
    return '∞';
  }
  if (seconds < 60) return `${Math.floor(seconds)}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ${Math.floor(seconds % 60)}s`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ${minutes % 60}m`;
  const days = Math.floor(hours / 24);
  return `${days}d ${hours % 24}h`;
}

export function formatRatio(ratio: number): string {
  return Number.isFinite(ratio) ? ratio.toFixed(2) : '∞';
}
