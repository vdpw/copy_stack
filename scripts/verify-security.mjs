import { readFile, readdir } from "node:fs/promises";
import { resolve } from "node:path";

const projectRoot = resolve(import.meta.dirname, "..");
const failures = [];

function check(condition, message) {
  if (!condition) {
    failures.push(message);
  }
}

function parseVersion(value) {
  const match = String(value).match(/(\d+)\.(\d+)\.(\d+)/);
  return match ? match.slice(1).map(Number) : null;
}

function versionAtLeast(value, minimum) {
  const actual = parseVersion(value);
  const required = parseVersion(minimum);
  if (!actual || !required) {
    return false;
  }

  for (let index = 0; index < required.length; index += 1) {
    if (actual[index] !== required[index]) {
      return actual[index] > required[index];
    }
  }
  return true;
}

const packageJson = JSON.parse(
  await readFile(resolve(projectRoot, "package.json"), "utf8")
);
const tauriConfig = JSON.parse(
  await readFile(resolve(projectRoot, "src-tauri/tauri.conf.json"), "utf8")
);
const cargoToml = await readFile(
  resolve(projectRoot, "src-tauri/Cargo.toml"),
  "utf8"
);
const ciWorkflow = await readFile(
  resolve(projectRoot, ".github/workflows/ci.yml"),
  "utf8"
);
const releaseWorkflow = await readFile(
  resolve(projectRoot, ".github/workflows/release.yml"),
  "utf8"
);

const security = tauriConfig.app?.security;
const productionCsp = security?.csp;
check(
  packageJson.packageManager === "pnpm@10.33.0",
  "The audited pnpm toolchain must stay pinned."
);
check(
  typeof productionCsp === "string" && productionCsp.length > 0,
  "Production CSP must be enabled."
);
check(
  !productionCsp?.includes("'unsafe-eval'") &&
    !productionCsp?.includes("'unsafe-inline'"),
  "Production CSP must not allow unsafe script or style execution."
);
check(
  productionCsp?.includes("object-src 'none'") &&
    productionCsp?.includes("frame-ancestors 'none'") &&
    productionCsp?.includes("base-uri 'none'"),
  "Production CSP must disable objects, embedding, and base URL injection."
);
check(
  !/img-src[^;]*\bdata:/i.test(productionCsp ?? ""),
  "Production CSP must not allow inline data-image payloads."
);
check(
  security?.assetProtocol?.enable === false &&
    Array.isArray(security?.assetProtocol?.scope) &&
    security.assetProtocol.scope.length === 0,
  "The unused asset protocol must stay disabled with an empty scope."
);
check(
  security?.freezePrototype === true,
  "Tauri IPC prototype freezing must remain enabled."
);

const capabilitiesDirectory = resolve(projectRoot, "src-tauri/capabilities");
const capabilityFiles = (await readdir(capabilitiesDirectory)).filter(name =>
  name.endsWith(".json")
);
check(
  JSON.stringify([...capabilityFiles].sort()) ===
    JSON.stringify(["main.json"]),
  "Exactly the main-window capability file must be enabled."
);

const capabilitiesByName = new Map();
for (const fileName of capabilityFiles) {
  const capability = JSON.parse(
    await readFile(resolve(capabilitiesDirectory, fileName), "utf8")
  );
  capabilitiesByName.set(fileName, capability);
  const permissions = capability.permissions ?? [];
  check(
    !permissions.includes("core:default"),
    `${fileName} must not grant core:default.`
  );
  check(
    !permissions.some(permission => String(permission).includes("opener")),
    `${fileName} must not grant opener permissions.`
  );
}

const mainPermissions = capabilitiesByName.get("main.json")?.permissions ?? [];
const exactPermissions = (actual, expected) =>
  JSON.stringify([...actual].sort()) === JSON.stringify([...expected].sort());
check(
  exactPermissions(mainPermissions, [
    "core:event:allow-listen",
    "core:event:allow-unlisten",
    "allow-get-startup-error",
    "allow-get-copy-events-page",
    "allow-get-history-detail",
    "allow-delete-copy-event",
    "allow-clear-all-events",
    "allow-copy-to-clipboard",
    "allow-get-app-settings",
    "allow-get-safe-diagnostics",
    "allow-get-autostart-status",
    "allow-set-autostart-enabled",
    "allow-set-max-items",
    "allow-set-max-history-bytes",
    "allow-set-show-in-menu-bar",
    "allow-set-move-restored-item-to-top",
    "allow-set-compact-mode",
    "allow-set-language",
  ]),
  "The main capability allowlist changed; review and update the audited snapshot."
);
check(
  mainPermissions.includes("allow-get-startup-error") &&
    mainPermissions.includes("allow-get-copy-events-page") &&
    mainPermissions.includes("allow-get-history-detail") &&
    mainPermissions.includes("allow-get-app-settings") &&
    mainPermissions.includes("allow-set-autostart-enabled"),
  "The main window must have the audited history and settings page permissions."
);
check(
  Array.isArray(security?.capabilities) &&
    security.capabilities.length === 1 &&
    security.capabilities.includes("main-window"),
  "Tauri must explicitly enable only the main-window capability."
);

check(
  !cargoToml.includes("tauri-plugin-opener"),
  "The unused opener plugin must stay removed from Cargo.toml."
);
check(
  !Object.keys(packageJson.dependencies ?? {}).some(name =>
    name.includes("plugin-opener")
  ),
  "The unused opener plugin must stay removed from package.json."
);
check(
  versionAtLeast(packageJson.dependencies?.["@tauri-apps/api"], "2.11.1"),
  "@tauri-apps/api must meet the audited minimum version 2.11.1."
);
check(
  versionAtLeast(packageJson.devDependencies?.["@tauri-apps/cli"], "2.11.4"),
  "@tauri-apps/cli must meet the audited minimum version 2.11.4."
);
check(
  versionAtLeast(packageJson.devDependencies?.vite, "6.4.2"),
  "Vite must meet the audited minimum version 6.4.2."
);
check(
  /^tauri\s*=\s*\{[^}]*version\s*=\s*"=?2\.11\.4"/ms.test(cargoToml),
  "Rust Tauri must remain on the audited 2.11.4 release or be re-audited."
);
const tauriBuildVersion = cargoToml.match(
  /^tauri-build\s*=\s*\{[^}]*version\s*=\s*"([^"]+)"/ms
)?.[1];
check(
  versionAtLeast(tauriBuildVersion, "2.6.3"),
  "tauri-build must remain at 2.6.3 or newer for Tauri 2.11.4 compatibility."
);
for (const [name, workflow] of [
  ["CI", ciWorkflow],
  ["Release", releaseWorkflow],
]) {
  check(
    workflow.includes("macos-15") && workflow.includes("macos-15-intel"),
    `${name} must exercise native Apple Silicon and Intel macOS runners.`
  );
}

if (failures.length > 0) {
  for (const failure of failures) {
    console.error(`security-check: ${failure}`);
  }
  process.exitCode = 1;
} else {
  console.log("security-check: configuration and dependency guardrails passed");
}
