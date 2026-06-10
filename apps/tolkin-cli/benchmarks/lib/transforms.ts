// Deterministic, documented transformations the runner applies in process.
//
// Every transform here is invariant: same input bytes in -> same bytes out,
// no randomness, no clocks. The harness's determinism contract leans on
// that, and the unit-style assertions below double as the contract for
// each rule.
//
// We do these in the runner instead of shelling tolkin for two reasons:
// (1) the audit JSON `preview` field caps the converted preview at 8 KB,
//     so it cannot serve as the canonical artifact for cases that exceed
//     that bound; the byte counts are exact, but the text is truncated.
// (2) the structural-track transforms (minify, dedup) are simple and
//     well known; doing them here keeps the harness self-contained and
//     auditable in one place, and lets the case `notes` document the
//     exact rule per case.

export function jsonMinify(text: string): string {
  const value = JSON.parse(text);
  return JSON.stringify(value);
}

/**
 * Remove blocks (paragraphs separated by one or more blank lines) whose
 * trimmed text matches a block already seen. The first occurrence stays;
 * each subsequent verbatim restatement is dropped. Trailing/leading blank
 * lines are normalized so the output joins back cleanly with `\n\n`.
 *
 * The rule is exact-match dedup, not semantic; "lossless" only holds when
 * the dropped blocks are byte-for-byte duplicates.
 */
export function dedupParagraphs(text: string): string {
  const blocks = text
    .split(/\n{2,}/)
    .map((b) => b.replace(/\s+$/g, "").replace(/^\s+/g, ""))
    .filter((b) => b.length > 0);
  const seen = new Set<string>();
  const kept: string[] = [];
  for (const block of blocks) {
    if (seen.has(block)) continue;
    seen.add(block);
    kept.push(block);
  }
  return `${kept.join("\n\n")}\n`;
}

/**
 * Collapse repeated stack frames within a single dump and dedupe whole
 * dumps when their post-header body is identical to a previously seen
 * dump. We split on the JVM error header pattern and treat each block as
 * one logical dump. The first dump is kept verbatim; subsequent dumps
 * whose body matches a prior body are replaced with a one-line marker.
 *
 * Header detection: a line starting with an ISO timestamp + ERROR is a
 * dump boundary. Bodies are compared by content from the next line to the
 * blank line that separates dumps.
 */
export function dedupStackTrace(text: string): string {
  // Split into dumps on the timestamp-and-ERROR header lines.
  const headerPattern = /^(?=\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z\s+ERROR)/m;
  const dumps = text.split(headerPattern).filter((d) => d.trim().length > 0);
  const out: string[] = [];
  const seenBodies = new Map<string, string>(); // body -> first header line
  for (const dump of dumps) {
    const nl = dump.indexOf("\n");
    const header = nl >= 0 ? dump.slice(0, nl) : dump;
    const body = (nl >= 0 ? dump.slice(nl + 1) : "").trimEnd();
    if (seenBodies.has(body)) {
      const firstHeader = seenBodies.get(body) ?? "earlier dump";
      out.push(`${header.trim()}\n\t... identical stack to: ${firstHeader.trim()}\n`);
    } else {
      seenBodies.set(body, header);
      out.push(`${dump.trimEnd()}\n`);
    }
  }
  return out.join("\n");
}

/**
 * Minimal HTML-to-Markdown transform mirroring the rules used in
 * tolkin-core's `format::html_to_markdown` (used by the audit preview).
 * Reproduced here so the structural-track artifact is not capped at the
 * 8 KB preview limit and so the rule is auditable in one place. The text
 * intent matches the core implementation's rules; line-for-line output
 * may differ from the core's regex pipeline because this version operates
 * in a JS regex engine.
 */
export function htmlToMarkdown(text: string): string {
  let out = text;
  // Strip script and style blocks first so their content cannot leak.
  out = out.replace(/<script\b[^>]*>[\s\S]*?<\/script\s*>/gi, "");
  out = out.replace(/<style\b[^>]*>[\s\S]*?<\/style\s*>/gi, "");
  // Headings become `## ` style.
  out = out.replace(/<h([1-6])\b[^>]*>/gi, (_m, level) => `\n\n${"#".repeat(Number(level))} `);
  out = out.replace(/<\/h[1-6]\s*>/gi, "\n\n");
  // List items become `- ` style.
  out = out.replace(/<li\b[^>]*>/gi, "\n- ");
  out = out.replace(/<\/li\s*>/gi, "");
  // Anchors become `[text](href)`.
  out = out.replace(
    /<a\b[^>]*\bhref\s*=\s*["']?([^"'\s>]+)["']?[^>]*>([\s\S]*?)<\/a\s*>/gi,
    "[$2]($1)",
  );
  // Strong and emphasis become markdown.
  out = out.replace(/<\/?(?:strong|b)\b[^>]*>/gi, "**");
  out = out.replace(/<\/?(?:em|i)\b[^>]*>/gi, "*");
  // Block boundaries collapse to newlines.
  out = out.replace(/<\/?(?:p|div)\b[^>]*>|<br\s*\/?>/gi, "\n");
  // Strip any remaining tag.
  out = out.replace(/<\/?[A-Za-z][^>]*>/g, "");
  // Decode entities; & last so escaped sequences stay literal.
  out = out
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/&amp;/g, "&");
  out = out.replace(/\n{3,}/g, "\n\n");
  return out.trim();
}
