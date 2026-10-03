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

  expectScope(lines, tokenLines, 13, "enum", "storage.type.declaration.gof");
  expectScope(lines, tokenLines, 13, "JobState", "entity.name.type.gof");
  expectScope(lines, tokenLines, 21, "fn", "keyword.declaration.function.gof");
  expectScope(lines, tokenLines, 21, "Runner", "entity.name.type.gof");
  expectScope(lines, tokenLines, 21, "run", "entity.name.function.gof");
  expectScope(lines, tokenLines, 21, "Result", "support.type.builtin.gof");
  expectScope(lines, tokenLines, 22, "mut", "storage.modifier.gof");
  expectScope(lines, tokenLines, 22, "channel", "support.type.builtin.gof");
  expectScope(lines, tokenLines, 22, "channel", "support.function.builtin.gof", 1);
  expectScope(lines, tokenLines, 23, "select", "keyword.control.concurrent.gof");
  expectScope(lines, tokenLines, 24, "recv", "support.function.builtin.gof");
  expectScope(lines, tokenLines, 24, "timeout_token", "support.function.builtin.gof");
  expectScope(lines, tokenLines, 26, "await_result", "support.function.builtin.gof");
  expectScope(lines, tokenLines, 26, "go", "keyword.control.concurrent.gof");
  expectScope(lines, tokenLines, 27, "default", "keyword.control.concurrent.gof");
  expectScope(lines, tokenLines, 28, "Err", "constant.other.enum-variant.gof");
  expectScope(lines, tokenLines, 28, "Cancelled", "constant.other.enum-variant.gof");
  expectScope(lines, tokenLines, 30, "compute", "entity.name.function.gof");
  expectScope(lines, tokenLines, 31, "and", "keyword.operator.logical.gof");
  expectScope(lines, tokenLines, 31, "not", "keyword.operator.logical.gof");
  expectScope(lines, tokenLines, 31, "false", "constant.language.boolean.gof");
  expectScope(lines, tokenLines, 37, "read_lines", "support.function.builtin.gof");
  expectScope(lines, tokenLines, 38, "join", "support.function.builtin.gof");
  expectScope(lines, tokenLines, 39, "toml_parse", "support.function.builtin.gof");
  expectScope(lines, tokenLines, 42, "csv_stringify", "support.function.builtin.gof");
  expectScope(lines, tokenLines, 45, "# syntax smoke", "comment.line.number-sign.gof");
  expectScope(lines, tokenLines, 48, "run_process", "support.function.builtin.gof");
  expectScope(lines, tokenLines, 51, "template_render", "support.function.builtin.gof");
  expectScope(lines, tokenLines, 54, "read_stdin", "support.function.builtin.gof");
  expectScope(lines, tokenLines, 55, "read_stdin_lines", "support.function.builtin.gof");
  expectScope(lines, tokenLines, 60, "unix_seconds", "support.function.builtin.gof");
  expectScope(lines, tokenLines, 61, "unix_millis", "support.function.builtin.gof");
  expectScope(lines, tokenLines, 66, "yaml_parse", "support.function.builtin.gof");
  expectScope(lines, tokenLines, 71, "base64_encode", "support.function.builtin.gof");
  expectScope(lines, tokenLines, 72, "base64_decode", "support.function.builtin.gof");
  expectScope(lines, tokenLines, 77, "http_request", "support.function.builtin.gof");
  expectScope(lines, tokenLines, 82, "NetDeadline", "support.type.builtin.gof");
  expectScope(lines, tokenLines, 82, "deadline_after", "support.function.builtin.gof");
  expectScope(lines, tokenLines, 83, "Bytes", "support.type.builtin.gof");
  expectScope(lines, tokenLines, 83, "bytes_from_string", "support.function.builtin.gof");
  expectScope(lines, tokenLines, 84, "WriteStream", "support.type.builtin.gof");
  expectScope(lines, tokenLines, 84, "open_write_stream", "support.function.builtin.gof");
  expectScope(lines, tokenLines, 89, "ReadStream", "support.type.builtin.gof");
  expectScope(lines, tokenLines, 89, "open_read_stream", "support.function.builtin.gof");
  expectScope(lines, tokenLines, 92, "TcpListener", "support.type.builtin.gof");
  expectScope(lines, tokenLines, 92, "listen_tcp", "support.function.builtin.gof");
  expectScope(lines, tokenLines, 93, "SocketAddr", "support.type.builtin.gof");
  expectScope(lines, tokenLines, 94, "DuplexStream", "support.type.builtin.gof");
  expectScope(lines, tokenLines, 94, "connect_tcp", "support.function.builtin.gof");
  expectScope(lines, tokenLines, 95, "bytes_len", "support.function.builtin.gof");
  expectScope(lines, tokenLines, 127, "import", "keyword.control.import.gof");
  expectScope(lines, tokenLines, 127, "testing", "entity.name.namespace.gof");
  expectScope(lines, tokenLines, 129, "test", "keyword.declaration.test.gof");
  expectScope(lines, tokenLines, 129, "fn", "keyword.declaration.function.gof");
  expectScope(lines, tokenLines, 129, "smoke_test", "entity.name.function.gof");
  expectScope(lines, tokenLines, 129, "TestContext", "support.type.builtin.gof");
  expectScope(lines, tokenLines, 133, "fixture", "keyword.declaration.fixture.gof");
  expectScope(lines, tokenLines, 133, "module", "keyword.other.fixture-scope.gof");
  expectScope(lines, tokenLines, 133, "fn", "keyword.declaration.function.gof");
  expectScope(lines, tokenLines, 133, "shared_total", "entity.name.function.gof");
  expectScope(lines, tokenLines, 136, "fixture", "keyword.declaration.fixture.gof");
  expectScope(lines, tokenLines, 136, "test", "keyword.other.fixture-scope.gof");
  expectScope(lines, tokenLines, 136, "scratch_dir", "entity.name.function.gof");
  expectScope(lines, tokenLines, 136, "TestContext", "support.type.builtin.gof");
  expectScope(lines, tokenLines, 136, "TempDir", "support.type.builtin.gof");
});

test("highlights tuple syntax constructs", async () => {
  const { lines, tokenLines } = await tokenizeFixture();

  // Tuple annotations and literals: the member types scope as builtin
  // types and the literal members as decimal integers (plain
  // identifiers carry no dedicated scope in this grammar).
  expectScope(lines, tokenLines, 97, "int", "support.type.builtin.gof", 0);
  expectScope(lines, tokenLines, 98, "3", "constant.numeric.integer.decimal.gof");
  expectScope(lines, tokenLines, 98, "4", "constant.numeric.integer.decimal.gof");
});

test("highlights closure syntax constructs", async () => {
  const { lines, tokenLines } = await tokenizeFixture();

  // RFC 0047: the anonymous `fn (` expression keyword scopes as the
  // function declaration keyword; the `fn(int)` type annotation scopes
  // as a builtin type.
  expectScope(lines, tokenLines, 103, "fn", "keyword.declaration.function.gof", 0);
  expectScope(lines, tokenLines, 103, "int", "support.type.builtin.gof", 1);
  expectScope(lines, tokenLines, 105, "fn", "support.type.builtin.gof", 0);
  expectScope(lines, tokenLines, 105, "fn", "keyword.declaration.function.gof", 1);
});

test("highlights protocol and unsafe syntax constructs", async () => {
  const { lines, tokenLines } = await tokenizeFixture();

  // RFC 0044: `protocol Name:` scopes as declaration + type; `impl` as
  // the storage modifier; the generic bracket form keeps the fn keyword.
  expectScope(lines, tokenLines, 109, "protocol", "storage.type.declaration.gof", 0);
  expectScope(lines, tokenLines, 109, "Shape", "entity.name.type.gof", 0);
  expectScope(lines, tokenLines, 118, "impl", "storage.modifier.gof", 0);
  expectScope(lines, tokenLines, 124, "fn", "keyword.declaration.function.gof", 0);
  // RFC 0046: `unsafe` scopes as the storage modifier alongside mut/defer.
  expectScope(lines, tokenLines, 121, "unsafe", "storage.modifier.gof", 0);
});

test("highlights module export and const syntax constructs", async () => {
  const { lines, tokenLines } = await tokenizeFixture();

  // RFC 0053: the `module:` export block header scopes as a declaration.
  expectScope(lines, tokenLines, 1, "module", "keyword.declaration.exports.gof", 0);
  // RFC 0054: `const NAME:` scopes as declaration + named constant.
  expectScope(lines, tokenLines, 5, "const", "keyword.declaration.const.gof", 0);
  expectScope(lines, tokenLines, 5, "MAX_SIZE", "entity.name.constant.gof", 0);
  expectScope(lines, tokenLines, 6, "GREETING", "entity.name.constant.gof", 0);
});
