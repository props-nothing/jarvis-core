import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { checkDocuments, headingAnchors, localLinks } from "./check-docs.mjs";

function fixture(t, files) {
  const root = mkdtempSync(path.join(os.tmpdir(), "jarvis-doc-check-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  for (const [name, content] of Object.entries(files)) {
    const destination = path.join(root, name);
    mkdirSync(path.dirname(destination), { recursive: true });
    writeFileSync(destination, content);
  }
  return root;
}

test("links resolve with fragments, encoded spaces, nested parentheses and explicit aliases", (t) => {
  const root = fixture(t, {
    "README.md": '[one](docs/a%20b.md#memory--context)\n[two](<docs/a b.md#legacy>)\n[three](docs/name(1).md "title")\n[ref]: docs/a%20b.md#memory--context-1\n<img src="asset.svg">',
    "docs/a b.md": '# Memory & context\n# Memory & context\n<a id="legacy"></a>\n',
    "docs/name(1).md": "# File\n",
    "asset.svg": "<svg/>",
  });
  assert.deepEqual(checkDocuments(root, ["README.md", "docs/a b.md", "docs/name(1).md"]).errors, []);
});

test("broken targets, renamed headings and repository escapes fail independently", (t) => {
  const root = fixture(t, {
    "README.md": "[missing](missing.md)\n[anchor](present.md#removed)\n[escape](../elsewhere.md)\n[bad](%ZZ.md)",
    "present.md": "# Current\n",
  });
  const result = checkDocuments(root, ["README.md", "present.md"]);
  assert.equal(result.errors.length, 4);
  assert.match(result.errors[0], /README.md:1 missing target/);
  assert.match(result.errors[1], /README.md:2 missing anchor/);
  assert.match(result.errors[2], /README.md:3 link leaves repository/);
  assert.match(result.errors[3], /README.md:4 invalid URL encoding/);
});

test("canonical ADR collisions and mismatched names fail while historical redirects remain valid", (t) => {
  const root = fixture(t, {
    "docs/adr/0206-one.md": "\uFEFF# ADR-0206: One\n",
    "docs/adr/0206-two.md": "# ADR-0206: Two\n",
    "docs/adr/0207-three.md": "# ADR-0208: Three\n",
    "docs/adr/old.md": "# Historical ADR-0206\n[decision](0206-one.md)\n",
  });
  const result = checkDocuments(root, ["docs/adr/0206-one.md", "docs/adr/0206-two.md", "docs/adr/0207-three.md", "docs/adr/old.md"]);
  assert.equal(result.errors.length, 2);
  assert.match(result.errors[0], /duplicate ADR-0206/);
  assert.match(result.errors[1], /heading does not match filename/);
});

test("code examples and external links are excluded without losing following lines", () => {
  const ticks = String.fromCharCode(96);
  const markdown = [
    "# Doc",
    ticks.repeat(3) + "md",
    "[example](not-a-file.md)",
    ticks.repeat(3),
    ticks + "[inline](not-a-file.md)" + ticks,
    "[remote](https://example.invalid/missing)",
    "[real](real.md)",
  ].join("\n");
  assert.deepEqual(localLinks(markdown), [{ target: "real.md", line: 7 }]);
});

test("heading slugs retain Unicode, duplicate suffixes and code text", () => {
  const ticks = String.fromCharCode(96);
  const anchors = headingAnchors("# Café " + ticks + "run" + ticks + "\n# Café run\nTitle\n=====\n");
  assert.ok(anchors.has("café-run"));
  assert.ok(anchors.has("café-run-1"));
  assert.ok(anchors.has("title"));
  assert.ok(headingAnchors("\uFEFF# " + ticks + "run_id" + ticks + " and _status_\n").has("run_id-and-status"));
});

test("Windows line endings preserve headings and prose is not a reference definition", () => {
  assert.ok(headingAnchors("# Heading\r\n\r\n~~~text\r\n# Example\r\n~~~\r\n## Later\r\n").has("later"));
  assert.deepEqual(localLinks("[term]: risk 3, every call held.\r\n[valid]: file.md\r\n"), [{ target: "file.md", line: 2 }]);
});
