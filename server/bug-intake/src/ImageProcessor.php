<?php
declare(strict_types=1);

namespace NebulaeIntake;

/** Decodes, validates and re-encodes screenshots. Re-encoding drops EXIF and any trailing/embedded data. */
final class ImageProcessor
{
    private const FORMATS = ['image/png' => IMAGETYPE_PNG, 'image/jpeg' => IMAGETYPE_JPEG, 'image/webp' => IMAGETYPE_WEBP];

    public function __construct(private Config $config)
    {
    }

    /** @return array{bytes:string,mime:string,width:int,height:int} */
    public function sanitize(string $raw): array
    {
        $max = $this->config->int('MAX_SCREENSHOT_BYTES');
        if ($raw === '' || strlen($raw) > $max) {
            throw new ValidationError('screenshot_size', 'A screenshot is empty or larger than ' . intdiv($max, 1048576) . ' MiB.', 413);
        }
        $mime = (new \finfo(FILEINFO_MIME_TYPE))->buffer($raw) ?: '';
        if (!isset(self::FORMATS[$mime])) {
            throw new ValidationError('screenshot_type', 'Screenshots must be PNG, JPEG or WebP images.', 415);
        }
        $info = @getimagesizefromstring($raw);
        if ($info === false || $info[2] !== self::FORMATS[$mime]) {
            throw new ValidationError('screenshot_corrupt', 'A screenshot could not be read as a valid image.', 422);
        }
        [$w, $h] = [(int) $info[0], (int) $info[1]];
        if ($w < 1 || $h < 1 || $w > $this->config->int('MAX_DIMENSION') || $h > $this->config->int('MAX_DIMENSION')
            || $w * $h > $this->config->int('MAX_PIXELS')) {
            throw new ValidationError('screenshot_dimensions', 'A screenshot has unsupported dimensions.', 422);
        }
        $img = @imagecreatefromstring($raw);
        if ($img === false) {
            throw new ValidationError('screenshot_corrupt', 'A screenshot could not be decoded.', 422);
        }
        try {
            $bytes = $this->encode($img, $mime);
        } finally {
        }
        return ['bytes' => $bytes, 'mime' => $mime, 'width' => $w, 'height' => $h];
    }

    /** Returns bytes small enough for one Discord message, downscaling to JPEG if needed. */
    public function fitForDiscord(string $bytes, string $mime, int $limit): array
    {
        if (strlen($bytes) <= $limit) {
            return ['bytes' => $bytes, 'mime' => $mime, 'resized' => false];
        }
        $img = @imagecreatefromstring($bytes);
        if ($img === false) {
            throw new \RuntimeException('stored screenshot could not be decoded');
        }
        $scale = 1.0;
        try {
            for ($i = 0; $i < 8; $i++) {
                $scale *= 0.75;
                $w = max(1, (int) (imagesx($img) * $scale));
                $h = max(1, (int) (imagesy($img) * $scale));
                $small = imagescale($img, $w, $h);
                if ($small === false) {
                    continue;
                }
                $flat = imagecreatetruecolor($w, $h);
                imagefill($flat, 0, 0, imagecolorallocate($flat, 255, 255, 255));
                imagecopy($flat, $small, 0, 0, 0, 0, $w, $h);
                ob_start();
                imagejpeg($flat, null, 82);
                $out = (string) ob_get_clean();
                if (strlen($out) <= $limit) {
                    return ['bytes' => $out, 'mime' => 'image/jpeg', 'resized' => true];
                }
            }
        } finally {
        }
        throw new \RuntimeException('screenshot cannot be reduced to the Discord size limit');
    }

    private function encode(\GdImage $img, string $mime): string
    {
        ob_start();
        try {
            if ($mime === 'image/png') {
                imagesavealpha($img, true);
                imagepng($img, null, 6);
            } elseif ($mime === 'image/jpeg') {
                imagejpeg($img, null, 90);
            } else {
                imagewebp($img, null, 90);
            }
        } finally {
            $out = (string) ob_get_clean();
        }
        if ($out === '') {
            throw new ValidationError('screenshot_corrupt', 'A screenshot could not be processed.', 422);
        }
        return $out;
    }
}
