import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, realpathSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

// Offline structural checks only. Historical claims and remote URLs need review.
export function markdownFiles(root) {
  const result = spawnSync("git", ["ls-files", "-z", "--cached", "--others", "--exclude-standard", "--", "*.md"], {
    cwd: root, encoding: "utf8", maxBuffer: 8 * 1024 * 1024,
  });
  if (result.error || result.status !== 0) throw new Error("git could not enumerate Markdown files");
  return [...new Set(result.stdout.split("\0").filter(Boolean))]
    .filter((file) => !file.startsWith("vendor/") && existsSync(path.join(root, file)))
    .sort();
}

function withoutFences(text) {
  let fence;
  return text.replace(/^\uFEFF/, "").replace(/\r\n/g, "\n").split("\n").map((line) => {
    const marker = line.match(/^\s{0,3}(\x60{3,}|~{3,})/);
    if (!fence && marker) { fence = marker[1]; return ""; }
    if (fence) {
      if (marker && marker[1][0] === fence[0] && marker[1].length >= fence.length &&
          /^\s*$/.test(line.slice(marker[0].length))) fence = undefined;
      return "";
    }
    return line;
  }).join("\n");
}

function entities(text) {
  return text.replace(/&amp;/g, "&").replace(/&lt;/g, "<").replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"').replace(/&#39;|&apos;/g, "'");
}

export function headingAnchors(markdown) {
  const text = withoutFences(markdown);
  const anchors = new Set();
  for (const match of text.matchAll(/\b(?:id|name)\s*=\s*["']([^"']+)["']/g)) anchors.add(match[1]);
  const used = new Set();
  function heading(value) {
    const code = [];
    value = value.replace(/(\x60+)(.*?)\1/g, (_, ticks, content) => {
      code.push(content);
      return "\u0000" + (code.length - 1) + "\u0000";
    });
    const plain = entities(value.replace(/<[^>]*>/g, "").replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
      .replace(/[\x60*~]/g, "").replace(/(^|[^\p{L}\p{N}])_+(.+?)_+(?=$|[^\p{L}\p{N}])/gu, "$1$2")
      .replace(/\u0000(\d+)\u0000/g, (_, index) => code[Number(index)])).trim().toLowerCase();
    const base = plain.replace(/[^\p{L}\p{M}\p{N}_\s-]/gu, "").replace(/\s/g, "-");
    let slug = base;
    for (let suffix = 1; used.has(slug); suffix++) slug = base + "-" + suffix;
    used.add(slug);
    anchors.add(slug);
  }
  const lines = text.split("\n");
  for (let index = 0; index < lines.length; index++) {
    const atx = lines[index].match(/^\s{0,3}#{1,6}\s+(.+?)(?:\s+#+\s*)?$/);
    if (atx) heading(atx[1]);
    else if (index > 0 && /^\s{0,3}(?:=+|-+)\s*$/.test(lines[index]) &&
             lines[index - 1].trim() && !/^\s*(?:>|[-*+])\s/.test(lines[index - 1])) heading(lines[index - 1]);
  }
  return anchors;
}

function destination(text, offset) {
  let index = offset;
  while (index < text.length && /\s/.test(text[index])) index++;
  if (text[index] === "<") {
    const end = text.indexOf(">", index + 1);
    return end < 0 ? "" : text.slice(index + 1, end);
  }
  const start = index;
  let depth = 0;
  while (index < text.length) {
    if (text[index] === "\\") { index += 2; continue; }
    if (/\s/.test(text[index])) break;
    if (text[index] === "(") depth++;
    if (text[index] === ")" && depth-- === 0) break;
    index++;
  }
  return text.slice(start, index).replace(/\\([ ()#])/g, "$1");
}

export function localLinks(markdown) {
  const text = withoutFences(markdown).replace(/(\x60+)[^\n]*?\1/g, (match) => " ".repeat(match.length));
  const links = [];
  function add(target, offset) {
    if (!target || /^(?:[a-z][a-z\d+.-]*:|\/\/)/i.test(target)) return;
    links.push({ target: entities(target), line: text.slice(0, offset).split("\n").length });
  }
  for (const match of text.matchAll(/\]\(/g)) add(destination(text, match.index + 2), match.index);
  for (const match of text.matchAll(/^\s{0,3}\[([^\]\n]+)\]:\s*(<[^>\n]+>|[^\s]+)(?:[^\S\n]+(?:"[^"\n]*"|'[^'\n]*'|\([^\n]*\)))?[^\S\n]*$/gm)) {
    if (match[1].trim()) add(match[2].startsWith("<") ? match[2].slice(1, -1) : match[2], match.index);
  }
  for (const match of text.matchAll(/<(?:a|img)\b[^>]*?\b(?:href|src)\s*=\s*["']([^"']+)["'][^>]*>/gi)) add(match[1], match.index);
  return links;
}

export function checkDocuments(root, files = markdownFiles(root)) {
  root = realpathSync(root);
  const errors = [];
  const cache = new Map();
  const decisions = new Map();
  let links = 0;
  function read(file) {
    if (!cache.has(file)) cache.set(file, readFileSync(file, "utf8"));
    return cache.get(file);
  }
  function outside(file) {
    const relative = path.relative(root, file);
    return relative === ".." || relative.startsWith(".." + path.sep) || path.isAbsolute(relative);
  }
  for (const file of files) {
    const absolute = path.join(root, file);
    const text = read(absolute);
    if (/^docs\/adr\//.test(file.replaceAll("\\", "/"))) {
      const decision = text.replace(/^\uFEFF/, "").match(/^# ADR-(\d{4}):/m);
      if (decision) {
        if (decisions.has(decision[1])) errors.push(file + ":1 duplicate ADR-" + decision[1] + " (" + decisions.get(decision[1]) + ")");
        decisions.set(decision[1], file);
        if (!path.basename(file).startsWith(decision[1] + "-")) errors.push(file + ":1 ADR heading does not match filename");
      }
    }
    for (const link of localLinks(text)) {
      links++;
      const prefix = file + ":" + link.line + " ";
      let target, fragment;
      try {
        const hash = link.target.indexOf("#");
        target = decodeURIComponent((hash < 0 ? link.target : link.target.slice(0, hash)).split("?")[0]);
        fragment = hash < 0 ? "" : decodeURIComponent(link.target.slice(hash + 1));
      } catch { errors.push(prefix + "invalid URL encoding: " + link.target); continue; }
      const resolved = target ? path.resolve(target.startsWith("/") ? root : path.dirname(absolute), target.startsWith("/") ? "." + target : target) : absolute;
      if (outside(resolved)) { errors.push(prefix + "link leaves repository: " + link.target); continue; }
      if (!existsSync(resolved)) { errors.push(prefix + "missing target: " + link.target); continue; }
      if (outside(realpathSync(resolved))) { errors.push(prefix + "link resolves outside repository: " + link.target); continue; }
      if (fragment && /\.md$/i.test(resolved) && statSync(resolved).isFile() && !headingAnchors(read(resolved)).has(fragment)) {
        errors.push(prefix + "missing anchor: " + link.target);
      }
    }
  }
  return { files: files.length, links, decisions: decisions.size, errors };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2);
  if (args.length && (args.length !== 2 || args[0] !== "--root")) {
    console.error("Usage: node scripts/check-docs.mjs [--root repository]");
    process.exitCode = 2;
  } else {
    try {
      const result = checkDocuments(path.resolve(args[1] ?? fileURLToPath(new URL("..", import.meta.url))));
      for (const error of result.errors) console.error(error);
      console.log(result.files + " Markdown files, " + result.links + " local links, " + result.decisions + " canonical ADRs; " + result.errors.length + " errors");
      process.exitCode = result.errors.length ? 1 : 0;
    } catch (error) {
      console.error(error.message);
      process.exitCode = 1;
    }
  }
}
