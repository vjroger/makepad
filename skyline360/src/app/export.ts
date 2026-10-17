/**
 * Encoders and downloads for a captured HDRI.
 */
import * as THREE from 'three';
import { GLTFExporter } from 'three/addons/exporters/GLTFExporter.js';
import type { HdrCapture } from '../render/capture';
import { encodeScanlineRLE, hdrHeader, rgbeFromFloats } from '../core/rgbe';
import { exrHeader, exrScanline, type ExrPixelType } from '../core/exr';
import { accumulatePreview, type PreviewImage } from '../render/preview';

export interface RenderMetadata {
  app: string;
  version: string;
  renderedAt: string;
  latitude: number;
  longitude: number;
  eyeHeight: number;
  heading: number;
  sunAzimuth: number;
  sunElevation: number;
  turbidity: number;
  sunIntensity: number;
  radius: number;
  width: number;
  height: number;
  dataSources: string[];
  demo: boolean;
}

/** Rows per strip: keeps each readback under ~32 MB even at 8k. */
export function stripRows(width: number): number {
  return Math.max(16, Math.min(512, Math.floor(8_000_000 / (width * 4))));
}

export type Progress = (fraction: number, label: string) => void;

/** Encode the capture as a Radiance .hdr (RGBE, RLE). */
export function encodeCaptureHdr(cap: HdrCapture, meta: RenderMetadata, progress?: Progress): Uint8Array[] {
  const parts: Uint8Array[] = [
    hdrHeader({
      width: cap.width,
      height: cap.height,
      comments: [
        `${meta.app} ${meta.version}`,
        `location ${meta.latitude.toFixed(6)},${meta.longitude.toFixed(6)} eye ${meta.eyeHeight} m heading ${meta.heading}`,
        `sun azimuth ${meta.sunAzimuth} elevation ${meta.sunElevation} turbidity ${meta.turbidity} intensity ${meta.sunIntensity}`,
      ],
    }),
  ];
  const rows = stripRows(cap.width);
  const scan = new Uint8Array(cap.width * 4);
  for (let y = 0; y < cap.height; y += rows) {
    const strip = cap.readStrip(y, rows);
    const n = Math.min(rows, cap.height - y);
    for (let r = 0; r < n; r++) {
      rgbeFromFloats(strip, r * cap.width * 4, cap.width, 4, scan);
      parts.push(encodeScanlineRLE(scan, cap.width));
    }
    progress?.((y + n) / cap.height, 'Encoding .hdr');
  }
  return parts;
}

/** Encode the capture as OpenEXR (half or float, uncompressed) with metadata attributes. */
export function encodeCaptureExr(cap: HdrCapture, meta: RenderMetadata, type: ExrPixelType, progress?: Progress): Uint8Array[] {
  const attributes: Record<string, string> = {
    comments: `${meta.app} ${meta.version}`,
    latitude: meta.latitude.toFixed(6),
    longitude: meta.longitude.toFixed(6),
    eyeHeight: String(meta.eyeHeight),
    heading: String(meta.heading),
    sunAzimuth: String(meta.sunAzimuth),
    sunElevation: String(meta.sunElevation),
    turbidity: String(meta.turbidity),
    sunIntensity: String(meta.sunIntensity),
    dataSources: meta.dataSources.join(' | '),
  };
  const { header } = exrHeader({ width: cap.width, height: cap.height, type, attributes });
  const parts: Uint8Array[] = [header];
  const rows = stripRows(cap.width);
  for (let y = 0; y < cap.height; y += rows) {
    const strip = cap.readStrip(y, rows);
    const n = Math.min(rows, cap.height - y);
    for (let r = 0; r < n; r++) parts.push(exrScanline(y + r, strip, r * cap.width * 4, cap.width, 4, type));
    progress?.((y + n) / cap.height, `Encoding .exr (${type})`);
  }
  return parts;
}

/** Build a reduced float copy for the on-screen previews (≤ 2048 px wide). */
export function buildPreview(cap: HdrCapture, maxWidth = 2048, progress?: Progress): PreviewImage {
  const factor = Math.max(1, Math.ceil(cap.width / maxWidth));
  const preview: PreviewImage = {
    width: Math.floor(cap.width / factor),
    height: Math.floor(cap.height / factor),
    data: new Float32Array(Math.floor(cap.width / factor) * Math.floor(cap.height / factor) * 3),
  };
  const rows = stripRows(cap.width);
  for (let y = 0; y < cap.height; y += rows) {
    const strip = cap.readStrip(y, rows);
    const n = Math.min(rows, cap.height - y);
    accumulatePreview(preview, strip, cap.width, y, n, factor);
    progress?.((y + n) / cap.height, 'Building preview');
  }
  return preview;
}

/** Trigger a browser download. */
export function download(parts: BlobPart[], filename: string, mime = 'application/octet-stream'): void {
  const blob = new Blob(parts, { type: mime });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 30000);
}

/** Export the scene meshes as a binary glTF. */
export async function exportGlb(group: THREE.Object3D): Promise<ArrayBuffer> {
  const exporter = new GLTFExporter();
  const result = await exporter.parseAsync(group, { binary: true, onlyVisible: true });
  if (result instanceof ArrayBuffer) return result;
  return new TextEncoder().encode(JSON.stringify(result)).buffer as ArrayBuffer;
}

export function sourcesText(meta: RenderMetadata): string {
  return [
    `${meta.app} ${meta.version} — data sources for this render`,
    `Rendered ${meta.renderedAt}`,
    `Location ${meta.latitude.toFixed(6)}, ${meta.longitude.toFixed(6)}; radius ${meta.radius} m`,
    '',
    ...meta.dataSources,
    '',
    'The HDRI is a Produced Work under the Open Database License where OpenStreetMap or Overture data was used.',
    'Keep this notice with the image when you publish it.',
  ].join('\n');
}
