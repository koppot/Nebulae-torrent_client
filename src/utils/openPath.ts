export const normalizeOpenPath = (source: string): string => {
  if (!/^file:/i.test(source)) return source;

  try {
    const url = new URL(source);
    if (url.protocol !== 'file:') return source;

    let path = decodeURIComponent(url.pathname);
    if (/^\/[a-z]:\//i.test(path)) {
      path = path.slice(1);
    }
    if (url.hostname && url.hostname.toLowerCase() !== 'localhost') {
      path = `//${url.hostname}${path}`;
    }
    return path;
  } catch {
    return source;
  }
};

export const readTorrentFileAsDataUrl = (file: File): Promise<string> =>
  new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      if (typeof reader.result !== 'string') {
        reject(new Error(`Could not read ${file.name}`));
        return;
      }
      const comma = reader.result.indexOf(',');
      if (comma < 0 || !reader.result.slice(0, comma).toLowerCase().endsWith(';base64')) {
        reject(new Error(`Could not encode ${file.name} as torrent metainfo`));
        return;
      }
      resolve(
        `data:application/x-bittorrent;name=${encodeURIComponent(file.name)};base64,${reader.result.slice(comma + 1)}`,
      );
    };
    reader.onerror = () => reject(reader.error ?? new Error(`Could not read ${file.name}`));
    reader.readAsDataURL(file);
  });

export const openPathKey = (
  source: string,
  isWindows =
    typeof navigator !== 'undefined' && /windows/i.test(navigator.userAgent),
): string => {
  const normalized = source.replace(/\\/g, '/');
  return isWindows ? normalized.toLowerCase() : normalized;
};
