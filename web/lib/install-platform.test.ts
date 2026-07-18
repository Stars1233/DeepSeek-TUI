import { describe, expect, it } from "vitest";
import { detectFromBrowserSignals } from "./install-platform";

describe("install platform detection", () => {
  const frozenWindowsUa =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36";

  it("uses User-Agent Client Hints for native Windows ARM64", () => {
    expect(
      detectFromBrowserSignals(frozenWindowsUa, {
        architecture: "arm",
        bitness: "64",
      }),
    ).toBe("windows-arm64");
  });

  it("keeps frozen Windows user agents on x64 without an ARM hint", () => {
    expect(detectFromBrowserSignals(frozenWindowsUa)).toBe("windows-x64");
  });

  it("retains the legacy ARM token fallback", () => {
    expect(detectFromBrowserSignals("Mozilla/5.0 (Windows NT 10.0; ARM64)")).toBe(
      "windows-arm64",
    );
  });
});
