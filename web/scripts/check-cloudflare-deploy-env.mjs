#!/usr/bin/env node
/**
 * check-cloudflare-deploy-env.mjs - fail fast when the GitHub deploy job is
 * missing Cloudflare credentials.
 *
 * The actual deploy still belongs to Wrangler/OpenNext. This script only makes
 * the common GitHub Actions failure mode obvious before the expensive build
 * starts.
 */

const required = [
  {
    name: "CLOUDFLARE_ACCOUNT_ID",
    source: "repository variable",
    expected: "Settings > Secrets and variables > Actions > Variables",
    validate(value) {
      return /^[a-f0-9]{32}$/i.test(value);
    },
    detail: "expected the 32-character Cloudflare account id",
  },
  {
    name: "CLOUDFLARE_API_TOKEN",
    source: "repository secret",
    expected: "Settings > Secrets and variables > Actions > Secrets",
    validate(value) {
      return value.length >= 20;
    },
    detail: "expected a non-empty Cloudflare API token",
  },
];

const placeholderPattern = /^(changeme|replace(_with)?|todo|example|dummy|null|undefined)$/i;
const failures = [];
const preflight = process.argv.includes("--preflight");

for (const item of required) {
  const value = (process.env[item.name] ?? "").trim();
  if (!value || placeholderPattern.test(value)) {
    failures.push({
      item,
      reason: `${item.name} is not set`,
    });
    continue;
  }

  if (!item.validate(value)) {
    failures.push({
      item,
      reason: `${item.name} is set but does not look valid`,
    });
  }
}

if (failures.length > 0) {
  if (preflight) {
    console.log(
      "[check-cloudflare-deploy-env] PREFLIGHT - deploy credentials are intentionally unavailable in this environment.",
    );
    console.log(
      `[check-cloudflare-deploy-env] ${failures.length} credential input(s) must be supplied by the protected manual deploy job.`,
    );
    console.log("[check-cloudflare-deploy-env] Wrangler deploy was not started.");
    process.exit(0);
  }

  console.error("[check-cloudflare-deploy-env] FAIL - Cloudflare deploy configuration is incomplete.");
  for (const failure of failures) {
    const { item, reason } = failure;
    console.error("");
    console.error(`- ${reason}`);
    console.error(`  Configure ${item.name} as a GitHub ${item.source}.`);
    console.error(`  Location: ${item.expected}.`);
    console.error(`  Hint: ${item.detail}.`);
  }
  console.error("");
  console.error("Wrangler deploy was not started.");
  process.exit(1);
}

console.log("[check-cloudflare-deploy-env] OK - Cloudflare deploy environment is present.");
