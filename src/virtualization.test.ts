import { describe, expect, it } from "vitest";
import { visibleRange } from "./virtualization";

describe("visibleRange", () => {
  it("keeps a large list virtual", () => {
    const range = visibleRange(500_000, 42, 84_000, 700, 8);
    expect(range.start).toBeGreaterThan(0);
    expect(range.end - range.start).toBeLessThan(50);
  });

  it("handles an empty list", () => {
    expect(visibleRange(0, 42, 0, 500, 5)).toEqual({ start: 0, end: 0 });
  });
});
