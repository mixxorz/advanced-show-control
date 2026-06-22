import { describe, expect, it } from "vitest";
import { formatSessionWindowTitle } from "./sessionTitle";

describe("formatSessionWindowTitle", () => {
  it("formats an untitled clean session", () => {
    expect(formatSessionWindowTitle("Untitled", false)).toBe(
      "Advanced Show Control - Untitled",
    );
  });

  it("adds a dirty marker", () => {
    expect(formatSessionWindowTitle("Tour Prep.adsc", true)).toBe(
      "Advanced Show Control - Tour Prep.adsc *",
    );
  });
});
