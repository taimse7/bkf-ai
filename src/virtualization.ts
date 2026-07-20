export interface VisibleRange {
  start: number;
  end: number;
}

export function visibleRange(
  total: number,
  rowHeight: number,
  scrollTop: number,
  viewportHeight: number,
  overscan: number,
): VisibleRange {
  if (total <= 0 || rowHeight <= 0) return { start: 0, end: 0 };
  const first = Math.floor(Math.max(0, scrollTop) / rowHeight);
  const visibleCount = Math.ceil(Math.max(0, viewportHeight) / rowHeight);
  const start = Math.min(total, Math.max(0, first - overscan));
  const end = Math.min(total, first + visibleCount + overscan);
  return { start, end: Math.max(start, end) };
}
