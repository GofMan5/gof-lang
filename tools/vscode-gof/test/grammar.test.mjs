import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import textmate from "vscode-textmate";
import oniguruma from "vscode-oniguruma";

const { Registry, parseRawGrammar } = textmate;

const require = createRequire(import.meta.url);
const __dirname = path.dirname(fileURLToPath(import.meta.url));
const grammarPath = path.resolve(__dirname, "..", "syntaxes", "gof.tmLanguage.json");
const fixturePath = path.resolve(__dirname, "fixtures", "showcase.gof");

let grammarPromise;

async function loadGrammar() {
  if (!grammarPromise) {
    grammarPromise = (async () => {
      const wasmPath = require.resolve("vscode-oniguruma/release/onig.wasm");
      const wasm = await readFile(wasmPath);
      await oniguruma.loadWASM(wasm);

      const registry = new Registry({
        onigLib: Promise.resolve({
          createOnigScanner(patterns) {
            return new oniguruma.OnigScanner(patterns);
          },
          createOnigString(text) {
            return new oniguruma.OnigString(text);
          }
        }),
        loadGrammar: async (scopeName) => {
          if (scopeName !== "source.gof") {
            return null;
          }
          const rawGrammar = await readFile(grammarPath, "utf8");
          return parseRawGrammar(rawGrammar, grammarPath);
        }
      });

      return registry.loadGrammar("source.gof");
    })();
  }

  return grammarPromise;
}

async function tokenizeFixture() {
  const grammar = await loadGrammar();
  const lines = (await readFile(fixturePath, "utf8")).split(/\r?\n/);
  let ruleStack = null;
  const tokenLines = [];

  for (const line of lines) {
    const result = grammar.tokenizeLine(line, ruleStack);
    tokenLines.push(result.tokens);
    ruleStack = result.ruleStack;
  }

  return { lines, tokenLines };
}

function scopesForFragment(lines, tokenLines, lineNumber, fragment, occurrence = 0) {
  const lineIndex = lineNumber - 1;
  const line = lines[lineIndex];
  let fromIndex = 0;
  let fragmentIndex = -1;

  for (let index = 0; index <= occurrence; index += 1) {
    fragmentIndex = line.indexOf(fragment, fromIndex);
    assert.notEqual(
      fragmentIndex,
      -1,
      `Could not find fragment "${fragment}" on line ${lineNumber} occurrence ${occurrence}`
    );
    fromIndex = fragmentIndex + fragment.length;
  }

  const token = tokenLines[lineIndex].find(
    (entry) => entry.startIndex <= fragmentIndex && entry.endIndex >= fragmentIndex + fragment.length
  );
  assert.ok(token, `Could not find token for "${fragment}" on line ${lineNumber}`);
  return token.scopes;
}

function expectScope(lines, tokenLines, lineNumber, fragment, expectedScope, occurrence = 0) {
  const scopes = scopesForFragment(lines, tokenLines, lineNumber, fragment, occurrence);
  assert.ok(
    scopes.includes(expectedScope),
    `Expected "${fragment}" on line ${lineNumber} to include scope "${expectedScope}", got ${scopes.join(", ")}`
  );
}

test("highlights core gof syntax constructs", async () => {
  const { lines, tokenLines } = await tokenizeFixture();

  expectScope(lines, tokenLines, 1, "module", "keyword.declaration.module.gof");
  expectScope(lines, tokenLines, 1, "sample", "entity.name.namespace.gof");
  expectScope(lines, tokenLines, 4, "enum", "storage.type.declaration.gof");
  expectScope(lines, tokenLines, 4, "JobState", "entity.name.type.gof");
  expectScope(lines, tokenLines, 12, "fn", "keyword.declaration.function.gof");
  expectScope(lines, tokenLines, 12, "Runner", "entity.name.type.gof");
  expectScope(lines, tokenLines, 12, "run", "entity.name.function.gof");
  expectScope(lines, tokenLines, 12, "Result", "support.type.builtin.gof");
  expectScope(lines, tokenLines, 13, "mut", "storage.modifier.gof");
  expectScope(lines, tokenLines, 13, "channel", "support.type.builtin.gof");
  expectScope(lines, tokenLines, 13, "channel", "support.function.builtin.gof", 1);
  expectScope(lines, tokenLines, 14, "ready", "string.quoted.double.gof");
  expectScope(lines, tokenLines, 15, "select", "keyword.control.concurrent.gof");
  expectScope(lines, tokenLines, 16, "recv", "support.function.builtin.gof");
  expectScope(lines, tokenLines, 16, "timeout_token", "support.function.builtin.gof");
  expectScope(lines, tokenLines, 17, "await_result", "support.function.builtin.gof");
  expectScope(lines, tokenLines, 17, "go", "keyword.control.concurrent.gof");
  expectScope(lines, tokenLines, 18, "default", "keyword.control.concurrent.gof");
  expectScope(lines, tokenLines, 19, "Err", "constant.other.enum-variant.gof");
  expectScope(lines, tokenLines, 19, "Cancelled", "constant.other.enum-variant.gof");
  expectScope(lines, tokenLines, 21, "compute", "entity.name.function.gof");
  expectScope(lines, tokenLines, 22, "and", "keyword.operator.logical.gof");
  expectScope(lines, tokenLines, 22, "not", "keyword.operator.logical.gof");
  expectScope(lines, tokenLines, 22, "false", "constant.language.boolean.gof");
  expectScope(lines, tokenLines, 28, "read_lines", "support.function.builtin.gof");
  expectScope(lines, tokenLines, 29, "join", "support.function.builtin.gof");
  expectScope(lines, tokenLines, 30, "toml_parse", "support.function.builtin.gof");
  expectScope(lines, tokenLines, 33, "csv_stringify", "support.function.builtin.gof");
  expectScope(lines, tokenLines, 36, "# syntax smoke", "comment.line.number-sign.gof");
});
