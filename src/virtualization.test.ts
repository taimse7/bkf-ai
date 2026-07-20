import { describe, expect, it } from "vitest";
import { visibleRange } from "./virtualization";

describe("visibleRange", () => {
  it("renders a bounded window from a 10,000-row library", () => {
    const range = visibleRange(10_000, 58, 9_500 * 58, 580, 8);
    expect(range.start).toBe(9_492);
    expect(range.end).toBe(9_518);
    expect(range.end - range.start).toBeLessThan(40);
  });

  it("clamps the final window", () => {
    expect(visibleRange(10_000, 58, 999_999, 580, 8)).toEqual({ start: 10_000, end: 10_000 });
  });
});
