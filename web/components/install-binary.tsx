"use client";

import { useEffect, useState } from "react";
import {
  detectFromBrowserSignals,
  type Arch,
  type UserAgentArchitecture,
} from "@/lib/install-platform";
import { InstallCodeBlock } from "./install-code-block";

function windowsSnippet(arch: "x64" | "arm64"): string {
  return `# PowerShell
$ErrorActionPreference = "Stop"
$dest = "$Env:USERPROFILE\\bin"
New-Item -ItemType Directory -Force $dest | Out-Null
$manifest = Invoke-WebRequest https://github.com/Hmbown/CodeWhale/releases/latest/download/codewhale-artifacts-sha256.txt

Invoke-WebRequest \`
  -Uri https://github.com/Hmbown/CodeWhale/releases/latest/download/codewhale-windows-${arch}.exe \`
  -OutFile "$dest\\codewhale.exe"
Invoke-WebRequest \`
  -Uri https://github.com/Hmbown/CodeWhale/releases/latest/download/codewhale-tui-windows-${arch}.exe \`
  -OutFile "$dest\\codewhale-tui.exe"

$expected = @{}
$manifest.Content -split "\`n" | ForEach-Object {
  $parts = $_.Trim() -split "\\s+"
  if ($parts.Length -ge 2) { $expected[$parts[1]] = $parts[0].ToUpperInvariant() }
}
if ((Get-FileHash "$dest\\codewhale.exe" -Algorithm SHA256).Hash -ne $expected["codewhale-windows-${arch}.exe"]) { throw "codewhale.exe checksum mismatch" }
if ((Get-FileHash "$dest\\codewhale-tui.exe" -Algorithm SHA256).Hash -ne $expected["codewhale-tui-windows-${arch}.exe"]) { throw "codewhale-tui.exe checksum mismatch" }

$Env:Path = "$dest;$Env:Path"`;
}

function windowsVerify(arch: "x64" | "arm64"): string {
  return `# PowerShell
$manifest = Invoke-WebRequest https://github.com/Hmbown/CodeWhale/releases/latest/download/codewhale-artifacts-sha256.txt
$expected = @{}
$manifest.Content -split "\`n" | ForEach-Object {
  $parts = $_.Trim() -split "\\s+"
  if ($parts.Length -ge 2) { $expected[$parts[1]] = $parts[0].ToUpperInvariant() }
}
if ((Get-FileHash "$Env:USERPROFILE\\bin\\codewhale.exe" -Algorithm SHA256).Hash -ne $expected["codewhale-windows-${arch}.exe"]) { throw "codewhale.exe checksum mismatch" }
if ((Get-FileHash "$Env:USERPROFILE\\bin\\codewhale-tui.exe" -Algorithm SHA256).Hash -ne $expected["codewhale-tui-windows-${arch}.exe"]) { throw "codewhale-tui.exe checksum mismatch" }`;
}

const SNIPPETS: Record<Arch, string> = {
  "macos-arm64": `curl -fsSL -O https://github.com/Hmbown/CodeWhale/releases/latest/download/codewhale-artifacts-sha256.txt
curl -fsSL -o codewhale \\
  https://github.com/Hmbown/CodeWhale/releases/latest/download/codewhale-macos-arm64
curl -fsSL -o codewhale-tui \\
  https://github.com/Hmbown/CodeWhale/releases/latest/download/codewhale-tui-macos-arm64
grep -E ' (codewhale|codewhale-tui)-macos-arm64$' codewhale-artifacts-sha256.txt | shasum -a 256 -c -
chmod +x codewhale codewhale-tui
xattr -d com.apple.quarantine codewhale codewhale-tui 2>/dev/null || true
sudo mv codewhale codewhale-tui /usr/local/bin/`,
  "macos-x64": `curl -fsSL -O https://github.com/Hmbown/CodeWhale/releases/latest/download/codewhale-artifacts-sha256.txt
curl -fsSL -o codewhale \\
  https://github.com/Hmbown/CodeWhale/releases/latest/download/codewhale-macos-x64
curl -fsSL -o codewhale-tui \\
  https://github.com/Hmbown/CodeWhale/releases/latest/download/codewhale-tui-macos-x64
grep -E ' (codewhale|codewhale-tui)-macos-x64$' codewhale-artifacts-sha256.txt | shasum -a 256 -c -
chmod +x codewhale codewhale-tui
xattr -d com.apple.quarantine codewhale codewhale-tui 2>/dev/null || true
sudo mv codewhale codewhale-tui /usr/local/bin/`,
  "linux-x64": `curl -fsSL -O https://github.com/Hmbown/CodeWhale/releases/latest/download/codewhale-artifacts-sha256.txt
curl -fsSL -o codewhale \\
  https://github.com/Hmbown/CodeWhale/releases/latest/download/codewhale-linux-x64
curl -fsSL -o codewhale-tui \\
  https://github.com/Hmbown/CodeWhale/releases/latest/download/codewhale-tui-linux-x64
grep -E ' (codewhale|codewhale-tui)-linux-x64$' codewhale-artifacts-sha256.txt | sha256sum -c -
chmod +x codewhale codewhale-tui
sudo mv codewhale codewhale-tui /usr/local/bin/`,
  "linux-arm64": `curl -fsSL -O https://github.com/Hmbown/CodeWhale/releases/latest/download/codewhale-artifacts-sha256.txt
curl -fsSL -o codewhale \\
  https://github.com/Hmbown/CodeWhale/releases/latest/download/codewhale-linux-arm64
curl -fsSL -o codewhale-tui \\
  https://github.com/Hmbown/CodeWhale/releases/latest/download/codewhale-tui-linux-arm64
grep -E ' (codewhale|codewhale-tui)-linux-arm64$' codewhale-artifacts-sha256.txt | sha256sum -c -
chmod +x codewhale codewhale-tui
sudo mv codewhale codewhale-tui /usr/local/bin/`,
  "windows-x64": windowsSnippet("x64"),
  "windows-arm64": windowsSnippet("arm64"),
};

const VERIFY: Record<Arch, string> = {
  "macos-arm64": `grep -E ' (codewhale|codewhale-tui)-macos-arm64$' codewhale-artifacts-sha256.txt | shasum -a 256 -c -`,
  "macos-x64": `grep -E ' (codewhale|codewhale-tui)-macos-x64$' codewhale-artifacts-sha256.txt | shasum -a 256 -c -`,
  "linux-x64": `grep -E ' (codewhale|codewhale-tui)-linux-x64$' codewhale-artifacts-sha256.txt | sha256sum -c -`,
  "linux-arm64": `grep -E ' (codewhale|codewhale-tui)-linux-arm64$' codewhale-artifacts-sha256.txt | sha256sum -c -`,
  "windows-x64": windowsVerify("x64"),
  "windows-arm64": windowsVerify("arm64"),
};

const LABELS: Record<Arch, string> = {
  "macos-arm64": "macOS · Apple Silicon",
  "macos-x64": "macOS · Intel",
  "linux-x64": "Linux · x64",
  "linux-arm64": "Linux · arm64",
  "windows-x64": "Windows · x64",
  "windows-arm64": "Windows · arm64",
};

interface NavigatorWithUserAgentData extends Navigator {
  userAgentData?: {
    getHighEntropyValues(hints: string[]): Promise<UserAgentArchitecture>;
  };
}

async function detect(): Promise<Arch> {
  if (typeof navigator === "undefined") return "macos-arm64";
  const browserNavigator = navigator as NavigatorWithUserAgentData;
  let architecture: UserAgentArchitecture | undefined;
  if (navigator.userAgent.toLowerCase().includes("win")) {
    try {
      architecture = await browserNavigator.userAgentData?.getHighEntropyValues([
        "architecture",
        "bitness",
      ]);
    } catch {
      // The manual platform buttons and frozen-UA fallback remain available.
    }
  }
  return detectFromBrowserSignals(navigator.userAgent, architecture);
}

interface Props {
  copyLabel?: string;
  copiedLabel?: string;
  verifyHeading?: string;
}

export function InstallBinary({ copyLabel, copiedLabel, verifyHeading = "Verify checksum" }: Props) {
  const [arch, setArch] = useState<Arch>("macos-arm64");

  useEffect(() => {
    let active = true;
    void detect().then((detected) => {
      if (active) setArch(detected);
    });
    return () => {
      active = false;
    };
  }, []);

  return (
    <div>
      <div className="flex flex-wrap gap-0 mb-3 hairline-t hairline-b hairline-l hairline-r">
        {(Object.keys(SNIPPETS) as Arch[]).map((a, i) => (
          <button
            key={a}
            onClick={() => setArch(a)}
            className={`px-3 py-1.5 font-mono text-[0.7rem] tracking-wider transition-colors ${
              i > 0 ? "hairline-l" : ""
            } ${arch === a ? "bg-ink text-paper" : "bg-paper hover:bg-paper-deep"}`}
          >
            {LABELS[a]}
          </button>
        ))}
      </div>

      <InstallCodeBlock cmd={SNIPPETS[arch]} copyLabel={copyLabel} copiedLabel={copiedLabel} />

      <div className="mt-4">
        <div className="eyebrow mb-2">{verifyHeading}</div>
        <InstallCodeBlock cmd={VERIFY[arch]} copyLabel={copyLabel} copiedLabel={copiedLabel} />
      </div>
    </div>
  );
}
