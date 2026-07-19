import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const CSS = readFileSync(new URL("../app/globals.css", import.meta.url), "utf8");
const TUI_TOKENS = readFileSync(
  new URL("../../crates/tui/src/palette/tokens.rs", import.meta.url),
  "utf8",
);

function rustRgb(name: string): string {
  const match = TUI_TOKENS.match(
    new RegExp(`pub const ${name}_RGB:[^=]+\\= \\((\\d+), (\\d+), (\\d+)\\)`),
  );
  if (!match) throw new Error(`Missing Rust RGB token: ${name}_RGB`);
  return `#${match
    .slice(1)
    .map((channel) => Number(channel).toString(16).padStart(2, "0"))
    .join("")}`;
}

function cssHex(name: string): string {
  const match = CSS.match(new RegExp(`--${name}:\\s*(#[0-9a-f]{6})`, "i"));
  if (!match) throw new Error(`Missing CSS token: --${name}`);
  return match[1].toLowerCase();
}

describe("Blue Stage public-surface contract", () => {
  it("shares the TUI stage, action, human, and structural dark tokens", () => {
    expect(cssHex("ocean-deep")).toBe(rustRgb("WHALE_BG"));
    expect(cssHex("action-on-dark")).toBe(rustRgb("WHALE_ACTION"));
    expect(cssHex("signal-gold")).toBe(rustRgb("WHALE_HUMAN"));
    expect(cssHex("ocean-current")).toBe(rustRgb("WHALE_ICE"));
  });

  it("shares the TUI light surface, text, and interaction tokens", () => {
    expect(cssHex("paper")).toBe(rustRgb("LIGHT_SURFACE"));
    expect(cssHex("paper-deep")).toBe(rustRgb("LIGHT_ELEVATED"));
    expect(cssHex("ink")).toBe(rustRgb("LIGHT_TEXT_BODY"));
    expect(cssHex("indigo")).toBe(rustRgb("LIGHT_ACTION"));
  });

  it("reserves Signal Gold for the whale while controls use action blue", () => {
    expect(CSS).toMatch(/\.site-wordmark svg \{ color: var\(--signal-gold\); \}/);
    expect(CSS).toMatch(/\.portal-button-primary[\s\S]*background: var\(--indigo-deep\)/);
    expect(CSS).toMatch(/\.nav-link::after[\s\S]*background: var\(--indigo\)/);
  });
});
