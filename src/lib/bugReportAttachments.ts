export const MAX_SCREENSHOTS = 5;
export const MAX_SCREENSHOT_BYTES = 10 * 1024 * 1024;
export const SCREENSHOT_EXTENSIONS = ['png', 'jpg', 'jpeg', 'gif', 'webp'] as const;

export const SCREENSHOT_LIMITS_TEXT = `PNG, JPEG, GIF or WebP, up to ${MAX_SCREENSHOTS} files, 10 MB each`;

export interface CandidateFile {
  name: string;
  size: number;
  type?: string;
}

export interface ScreenshotCheck<T extends CandidateFile> {
  accepted: T[];
  errors: string[];
}

function extensionOf(name: string): string {
  const dot = name.lastIndexOf('.');
  return dot < 0 ? '' : name.slice(dot + 1).toLowerCase();
}

export function formatMegabytes(bytes: number): string {
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** Checks newly chosen files against the type, size and count limits, given how
 * many screenshots are already attached. Every rejection yields its own message. */
export function checkScreenshots<T extends CandidateFile>(
  alreadyAttached: number,
  incoming: T[],
): ScreenshotCheck<T> {
  const accepted: T[] = [];
  const errors: string[] = [];
  for (const file of incoming) {
    const ext = extensionOf(file.name);
    const typeOk = file.type ? file.type.startsWith('image/') : true;
    if (!(SCREENSHOT_EXTENSIONS as readonly string[]).includes(ext) || !typeOk) {
      errors.push(`"${file.name}" is not a supported image (${SCREENSHOT_LIMITS_TEXT}).`);
      continue;
    }
    if (file.size <= 0) {
      errors.push(`"${file.name}" is empty.`);
      continue;
    }
    if (file.size > MAX_SCREENSHOT_BYTES) {
      errors.push(
        `"${file.name}" is ${formatMegabytes(file.size)}; the limit is ${formatMegabytes(MAX_SCREENSHOT_BYTES)} per screenshot.`,
      );
      continue;
    }
    if (alreadyAttached + accepted.length >= MAX_SCREENSHOTS) {
      errors.push(`"${file.name}" was not added: at most ${MAX_SCREENSHOTS} screenshots per report.`);
      continue;
    }
    accepted.push(file);
  }
  return { accepted, errors };
}
