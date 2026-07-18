export type Arch =
  | "macos-arm64"
  | "macos-x64"
  | "linux-x64"
  | "linux-arm64"
  | "windows-x64"
  | "windows-arm64";

export interface UserAgentArchitecture {
  architecture?: string;
  bitness?: string;
}

export function detectFromBrowserSignals(
  userAgent: string,
  userAgentArchitecture?: UserAgentArchitecture,
): Arch {
  const ua = userAgent.toLowerCase();
  if (ua.includes("win")) {
    const architecture = userAgentArchitecture?.architecture?.toLowerCase();
    const bitness = userAgentArchitecture?.bitness;
    if (
      architecture === "arm64" ||
      (architecture === "arm" && bitness === "64") ||
      ua.includes("aarch64") ||
      ua.includes("arm64")
    ) {
      return "windows-arm64";
    }
    return "windows-x64";
  }
  if (ua.includes("linux")) {
    if (ua.includes("aarch64") || ua.includes("arm64")) return "linux-arm64";
    return "linux-x64";
  }
  return "macos-arm64";
}
