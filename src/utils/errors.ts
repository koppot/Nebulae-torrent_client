// Shared error-message extraction. Tauri `invoke` rejections arrive as plain
// strings; native paths arrive as Error instances or objects with a message.
export function errorMessage(err: unknown, fallback = 'Unknown error'): string {
  if (typeof err === 'string') return err;
  if (err instanceof Error) return err.message;
  if (
    typeof err === 'object' &&
    err !== null &&
    'message' in err &&
    typeof (err as { message?: unknown }).message === 'string'
  ) {
    return (err as { message: string }).message;
  }
  return fallback;
}
