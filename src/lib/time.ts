// Timestamp formatting shared by every place that renders a Segment
// timestamp (spec Design Notes: "Offset là thuần hiển thị (AD-9): mọi nơi
// vẽ/export timestamp ... phải đi qua `src/lib/time.ts` thay vì tự cộng").
// Offset is display-only: it is never written back into a Segment or the DB
// (spec Always), and changing it never triggers an IPC call (spec I/O
// Matrix "Offset đổi").
import { settingsStore } from './stores/settings.svelte';

function clampSeconds(value: number): number {
  return Number.isFinite(value) ? Math.max(0, Math.round(value)) : 0;
}

function pad(value: number): string {
  return value.toString().padStart(2, '0');
}

/**
 * Format `sec` (+ `offsetSec`, default 0) as `MM:SS`, switching to
 * `HH:MM:SS` once the total reaches an hour (spec Always: "helper định dạng
 * `HH:MM:SS` (bỏ giờ khi < 1 giờ) cộng offset"). Non-finite or negative
 * input (after adding the offset) clamps to `00:00` rather than rendering a
 * negative or broken timestamp.
 */
export function formatTimestamp(sec: number, offsetSec = 0): string {
  const totalSeconds = clampSeconds(sec + offsetSec);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  return hours > 0
    ? `${pad(hours)}:${pad(minutes)}:${pad(seconds)}`
    : `${pad(minutes)}:${pad(seconds)}`;
}

/**
 * Reactive convenience wrapper over [`formatTimestamp`]: reads
 * `timestampOffsetSec` from `settingsStore` so every caller reflects an
 * offset change immediately, with no IPC call and no re-derivation of their
 * own (spec I/O Matrix "Offset đổi": "Giá trị định dạng cập nhật ngay,
 * không gọi IPC transcribe").
 */
export function displayTimestamp(sec: number): string {
  return formatTimestamp(sec, settingsStore.timestampOffsetSec);
}
