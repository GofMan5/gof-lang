import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const rootDir = path.resolve(__dirname, "..");
const packagePath = path.join(rootDir, "package.json");
const snippetsPath = path.join(rootDir, "snippets", "gof.code-snippets");
const iconPath = path.join(rootDir, "media", "gof-icon.png");

async function readPackageJson() {
  return JSON.parse(await readFile(packagePath, "utf8"));
}

function readPngDimensions(buffer) {
  const signature = buffer.subarray(0, 8).toString("hex");
  assert.equal(signature, "89504e470d0a1a0a", "icon must be a PNG file");
  return {
    width: buffer.readUInt32BE(16),
    height: buffer.readUInt32BE(20)
  };
}

test("package metadata is marketplace-ready", async () => {
  const manifest = await readPackageJson();

  assert.equal(manifest.main, "./extension.cjs");
  assert.equal(manifest.publisher, "gofman5");
  assert.equal(manifest.icon, "media/gof-icon.png");
  assert.deepEqual(manifest.galleryBanner, {
    color: "#0f172a",
    theme: "dark"
  });
  assert.ok(manifest.categories.includes("Programming Languages"));
  assert.ok(manifest.categories.includes("Snippets"));
  assert.ok(manifest.activationEvents.includes("onLanguage:gof"));
  assert.ok(manifest.activationEvents.includes("onCommand:gof.showDiagnosticsOutput"));
  assert.ok(manifest.activationEvents.includes("onCommand:gof.recheckActiveDocument"));
  assert.ok(manifest.files.includes("extension.cjs"));
  assert.ok(manifest.files.includes("lib/**"));
  assert.ok(
    manifest.contributes.commands.some((entry) => entry.command === "gof.showDiagnosticsOutput")
  );
  assert.ok(
    manifest.contributes.commands.some((entry) => entry.command === "gof.recheckActiveDocument")
  );
  assert.equal(manifest.contributes.configuration.properties["gof.diagnostics.enabled"].default, true);
  assert.equal(manifest.contributes.configuration.properties["gof.diagnostics.debounceMs"].default, 250);
  assert.equal(
    manifest.contributes.configuration.properties["gof.toolchain.useCargoFallback"].default,
    true
  );
  assert.ok(manifest.contributes.snippets.some((entry) => entry.path === "./snippets/gof.code-snippets"));
});

test("snippet file includes core gof templates", async () => {
  const snippets = JSON.parse(await readFile(snippetsPath, "utf8"));

  for (const key of [
    "Module Skeleton",
    "Function",
    "Struct",
    "Enum",
    "Match Result",
    "Select",
    "Task Join",
    "Process Capture",
    "Template Render",
    "Stdin Report",
    "Time Report",
    "Base64 Roundtrip",
    "YAML Config"
  ]) {
    assert.ok(snippets[key], `missing snippet "${key}"`);
    assert.ok(snippets[key].prefix, `snippet "${key}" must define a prefix`);
    assert.ok(Array.isArray(snippets[key].body), `snippet "${key}" must define a body array`);
  }
});

test("icon exists and is large enough for marketplace packaging", async () => {
  const icon = await readFile(iconPath);
  const { width, height } = readPngDimensions(icon);

  assert.ok(width >= 128, `icon width must be at least 128px, got ${width}`);
  assert.ok(height >= 128, `icon height must be at least 128px, got ${height}`);
});
