import { formatTimestamp } from './time';

export type CopyableTranscriptSegment = {
  startSec: number | null;
  endSec: number | null;
  kind: string;
  gapReason: string | null;
  text: string;
};

export type GapCopyLabels = {
  chunkFailed: string;
  disconnected: string;
  unknown: string;
};

/** Build the full plain-text transcript for clipboard copy, including gaps. */
export function formatTranscriptCopyText(
  segments: readonly CopyableTranscriptSegment[],
  offsetSec: number,
  labels: GapCopyLabels,
): string {
  return segments
    .map((segment) => {
      if (segment.kind !== 'gap') {
        return `[${formatTimestamp(segment.startSec ?? 0, offsetSec)}] ${segment.text}`;
      }
      const note = segment.gapReason === 'chunk_failed'
        ? labels.chunkFailed
        : segment.gapReason === 'disconnected'
          ? labels.disconnected
          : labels.unknown;
      return `[${formatTimestamp(segment.startSec ?? 0, offsetSec)}–${formatTimestamp(segment.endSec ?? 0, offsetSec)}] ${note}`;
    })
    .join('\n');
}
