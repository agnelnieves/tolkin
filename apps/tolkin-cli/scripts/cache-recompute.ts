#!/usr/bin/env bun
/**
 * Independent recompute for the `tolkin cache` reconciliation ritual (the
 * I2 pattern: an implementation of the spec from scratch, in a different
 * language, compared field for field against the product).
 *
 * Implements, without importing any product code:
 *   1. Claude Code log walk: $HOME/.claude/projects/<dir>/<file>.jsonl
 *   2. Within-file last-wins dedup on (message.id, requestId), synthetic
 *      models excluded, strict ISO-8601 Z timestamps only.
 *   3. Global cross-file merge: larger ts wins; tie on ts, larger
 *      output_tokens wins; full tie, the lexicographically first file path
 *      wins.
 *   4. Per-request retention grouped by (file stem, cwd), sorted by the
 *      tuple (ts, input, cache_read, write_5m, write_1h).
 *   5. The cache analysis: hit rate (overall and per UTC day vs the 0.5
 *      threshold), write churn (writes after a session's first write),
 *      the 5m/1h TTL counterfactual over real gaps with reads refreshing
 *      the TTL (re-write only when a gap STRICTLY exceeds the TTL), and
 *      cadence facts (intra-session gaps over 5m; inter-session gaps under
 *      1h within the same project; zero-cache-read sessions).
 *
 * Read-only by construction: this script opens log files for reading and
 * writes nothing anywhere.
 *
 * Usage: bun apps/tolkin-cli/scripts/cache-recompute.ts [--home <dir>]
 * Output: a JSON object shaped like the `cache` object of
 * `tolkin cache --global --json` (numeric fields; presentation strings like
 * the verdict are intentionally omitted from comparison).
 */

import { readdirSync, readFileSync, statSync } from "fs";
import { homedir } from "os";
import { join } from "path";

// ---------------------------------------------------------------------------
// Pricing (vendored constants, USD per MTok; null = vendor publishes none).
// Mirrors packages/tolkin-core/crates/core/src/pricing.rs at 2026-06 and the
// cache_rates fallback (missing cache rates fall back to the input rate).
// ---------------------------------------------------------------------------

type PriceRow = {
  input: number;
  cache_read: number | null;
  cache_write_5m: number | null;
  cache_write_1h: number | null;
};

const PRICES: Record<string, PriceRow> = {
  "claude-opus-4.8": { input: 5.0, cache_read: 0.5, cache_write_5m: 6.25, cache_write_1h: 10.0 },
  "claude-opus-4.7": { input: 5.0, cache_read: 0.5, cache_write_5m: 6.25, cache_write_1h: 10.0 },
  "claude-sonnet-4.6": { input: 3.0, cache_read: 0.3, cache_write_5m: 3.75, cache_write_1h: 6.0 },
  "claude-haiku-4.5": { input: 1.0, cache_read: 0.1, cache_write_5m: 1.25, cache_write_1h: 2.0 },
  "gpt-5.5": { input: 5.0, cache_read: 0.5, cache_write_5m: null, cache_write_1h: null },
  "gpt-5.4": { input: 2.5, cache_read: 0.25, cache_write_5m: null, cache_write_1h: null },
  "gpt-5.4-mini": { input: 0.75, cache_read: 0.075, cache_write_5m: null, cache_write_1h: null },
  "gpt-5.4-nano": { input: 0.2, cache_read: 0.02, cache_write_5m: null, cache_write_1h: null },
  "gemini-2.5-pro": { input: 1.25, cache_read: 0.125, cache_write_5m: null, cache_write_1h: null },
  "gemini-2.5-flash": { input: 0.3, cache_read: 0.03, cache_write_5m: null, cache_write_1h: null },
  "gemini-2.5-flash-lite": {
    input: 0.1,
    cache_read: 0.01,
    cache_write_5m: null,
    cache_write_1h: null,
  },
};

/** Mirrors usage::cost::normalize_model_id. */
function normalizeModelId(raw: string): string {
  let s = raw.trim().toLowerCase();
  // Strip trailing -YYYYMMDD (exactly 8 digits after the last hyphen).
  const idx = s.lastIndexOf("-");
  if (idx >= 0) {
    const tail = s.slice(idx + 1);
    if (tail.length === 8 && /^\d{8}$/.test(tail)) s = s.slice(0, idx);
  }
  // Collapse adjacent all-digit segments with a dot, left to right.
  const segments = s.split("-");
  if (segments.length < 2) return s;
  const out: string[] = [];
  for (const seg of segments) {
    const last = out[out.length - 1];
    const lastDigits = last !== undefined && last.length > 0 && /^\d+$/.test(last);
    const segDigits = seg.length > 0 && /^\d+$/.test(seg);
    if (lastDigits && segDigits) {
      out[out.length - 1] = `${last}.${seg}`;
    } else {
      out.push(seg);
    }
  }
  return out.join("-");
}

/** Mirrors usage::cost::cache_rates (input-rate fallbacks). */
function cacheRates(
  rawModel: string,
): { cache_read: number; cache_write_5m: number; cache_write_1h: number } | null {
  const row = PRICES[normalizeModelId(rawModel)];
  if (!row) return null;
  return {
    cache_read: row.cache_read ?? row.input,
    cache_write_5m: row.cache_write_5m ?? row.input,
    cache_write_1h: row.cache_write_1h ?? row.input,
  };
}

// ---------------------------------------------------------------------------
// Strict timestamp parsing (mirrors usage::claude_code::parse_iso8601:
// "YYYY-MM-DDTHH:MM:SS(.fff)Z" only; day capped at 31 regardless of month,
// second 60 tolerated; Hinnant's days_from_civil).
// ---------------------------------------------------------------------------

function daysFromCivil(y: number, m: number, d: number): number {
  const yy = m <= 2 ? y - 1 : y;
  const era = Math.floor((yy >= 0 ? yy : yy - 399) / 400);
  const yoe = yy - era * 400;
  const doy = Math.floor((153 * (m > 2 ? m - 3 : m + 9) + 2) / 5) + d - 1;
  const doe = yoe * 365 + Math.floor(yoe / 4) - Math.floor(yoe / 100) + doy;
  return era * 146097 + doe - 719468;
}

function civilFromDays(z0: number): [number, number, number] {
  const z = z0 + 719468;
  const era = Math.floor((z >= 0 ? z : z - 146096) / 146097);
  const doe = z - era * 146097;
  const yoe = Math.floor(
    (doe - Math.floor(doe / 1460) + Math.floor(doe / 36524) - Math.floor(doe / 146096)) / 365,
  );
  let y = yoe + era * 400;
  const doy = doe - (365 * yoe + Math.floor(yoe / 4) - Math.floor(yoe / 100));
  const mp = Math.floor((5 * doy + 2) / 153);
  const d = doy - Math.floor((153 * mp + 2) / 5) + 1;
  const m = mp < 10 ? mp + 3 : mp - 9;
  if (m <= 2) y += 1;
  return [y, m, d];
}

function parseIso8601Z(s: string): number | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(\.\d+)?Z$/.exec(s);
  if (!m) return null;
  const year = Number(m[1]);
  const month = Number(m[2]);
  const day = Number(m[3]);
  const hour = Number(m[4]);
  const minute = Number(m[5]);
  const second = Number(m[6]);
  if (month < 1 || month > 12 || day < 1 || day > 31) return null;
  if (hour > 23 || minute > 59 || second > 60) return null;
  const secs = daysFromCivil(year, month, day) * 86400 + hour * 3600 + minute * 60 + second;
  return secs < 0 ? null : secs;
}

function utcDayFromSecs(secs: number): string {
  const [y, m, d] = civilFromDays(Math.floor(secs / 86400));
  const pad = (n: number, w: number) => String(n).padStart(w, "0");
  return `${pad(y, 4)}-${pad(m, 2)}-${pad(d, 2)}`;
}

// ---------------------------------------------------------------------------
// Log walk + two-stage dedup
// ---------------------------------------------------------------------------

type Rec = {
  ts: number;
  input: number;
  read: number;
  w5: number;
  w1: number;
  output: number;
  cwd: string;
  model: string;
};

type Json = null | boolean | number | string | Json[] | { [k: string]: Json };

function asObj(x: Json | undefined): { [k: string]: Json } | null {
  return x !== null && x !== undefined && typeof x === "object" && !Array.isArray(x)
    ? (x as { [k: string]: Json })
    : null;
}

function asStr(x: Json | undefined): string | null {
  return typeof x === "string" ? x : null;
}

/** serde_json's as_u64 yields None for non-integers and negatives; the
 * product collapses those to 0, so mirror that exactly. */
function asU64(x: Json | undefined): number {
  return typeof x === "number" && Number.isInteger(x) && x >= 0 ? x : 0;
}

function extractRecord(
  v: Json,
): (Omit<Rec, "ts"> & { timestamp: string; messageId: string; requestId: string }) | null {
  const root = asObj(v);
  if (!root) return null;
  const message = asObj(root.message);
  if (!message) return null;
  const usage = asObj(message.usage);
  if (!usage) return null;
  const model = asStr(message.model);
  if (model === null || model === "<synthetic>") return null;
  const messageId = asStr(message.id);
  const requestId = asStr(root.requestId);
  const timestamp = asStr(root.timestamp);
  const cwd = asStr(root.cwd);
  if (messageId === null || requestId === null || timestamp === null || cwd === null) return null;
  const creationTotal = asU64(usage.cache_creation_input_tokens);
  // Exact product semantics: when the cache_creation KEY is present (even
  // as null or a non-object), the nested ephemeral fields are read with a
  // 0 default; only an ABSENT key falls back to cache_creation_input_tokens
  // dumped into the 5m bucket.
  let w5: number;
  let w1: number;
  if (Object.prototype.hasOwnProperty.call(usage, "cache_creation")) {
    const cc = asObj(usage.cache_creation);
    w5 = cc ? asU64(cc.ephemeral_5m_input_tokens) : 0;
    w1 = cc ? asU64(cc.ephemeral_1h_input_tokens) : 0;
  } else {
    w5 = creationTotal;
    w1 = 0;
  }
  return {
    input: asU64(usage.input_tokens),
    read: asU64(usage.cache_read_input_tokens),
    w5,
    w1,
    output: asU64(usage.output_tokens),
    cwd,
    model,
    timestamp,
    messageId,
    requestId,
  };
}

function parseFile(path: string): Map<string, Rec> {
  const dedup = new Map<string, Rec>();
  let text: string;
  try {
    text = readFileSync(path, "utf-8");
  } catch {
    return dedup;
  }
  for (const line of text.split("\n")) {
    if (line.trim() === "") continue;
    let v: Json;
    try {
      v = JSON.parse(line) as Json;
    } catch {
      continue;
    }
    const raw = extractRecord(v);
    if (!raw) continue;
    const ts = parseIso8601Z(raw.timestamp);
    if (ts === null) continue;
    const key = `${raw.messageId}\u0000${raw.requestId}`;
    // Last-wins within a file: Map.set overwrites the value.
    dedup.set(key, {
      ts,
      input: raw.input,
      read: raw.read,
      w5: raw.w5,
      w1: raw.w1,
      output: raw.output,
      cwd: raw.cwd,
      model: raw.model,
    });
  }
  return dedup;
}

function isDir(p: string): boolean {
  try {
    return statSync(p).isDirectory();
  } catch {
    return false;
  }
}

/**
 * Walk parent session files AND subagent transcripts:
 *   <projectsDir>/<slug>/<session>.jsonl                  (parent sessions)
 *   <projectsDir>/<slug>/<session>/subagents/agent-*.jsonl (subagent streams)
 *
 * Returns files in the order discovered (the caller sorts before merging).
 */
function walkClaudeFiles(projectsDir: string): string[] {
  const files: string[] = [];
  if (!isDir(projectsDir)) return files;
  for (const entry of readdirSync(projectsDir)) {
    const dir = join(projectsDir, entry);
    if (!isDir(dir)) continue;
    for (const f of readdirSync(dir)) {
      const child = join(dir, f);
      if (isDir(child)) {
        // <session_id>/ next to <session_id>.jsonl: look for subagents/.
        const subagents = join(child, "subagents");
        if (!isDir(subagents)) continue;
        for (const s of readdirSync(subagents)) {
          // Only `agent-<id>.jsonl` files; skip the `.meta.json` companions.
          if (s.startsWith("agent-") && s.endsWith(".jsonl")) {
            files.push(join(subagents, s));
          }
        }
        continue;
      }
      if (f.endsWith(".jsonl") && f !== ".jsonl") files.push(child);
    }
  }
  return files;
}

/**
 * Detect whether a path matches the subagent layout
 * `.../<parent_session_id>/subagents/agent-*.jsonl`. Returns the parent
 * session id when it does, null otherwise.
 */
function parentSessionIdFromPath(path: string): string | null {
  const segs = path.split("/");
  // [.., <parent>, subagents, agent-*.jsonl]
  if (segs.length < 3) return null;
  const last = segs[segs.length - 1];
  const dir = segs[segs.length - 2];
  if (dir !== "subagents") return null;
  if (!last.startsWith("agent-") || !last.endsWith(".jsonl")) return null;
  return segs[segs.length - 3];
}

type Session = {
  sessionId: string;
  projectKey: string;
  firstTs: number;
  lastTs: number;
  requests: Rec[];
  byModelInputSide: Map<string, number>;
};

function buildSessions(home: string): Session[] {
  const projectsDir = join(home, ".claude", "projects");
  const files = walkClaudeFiles(projectsDir).sort(); // lex order = tie-break order
  // Global merge: iterate files ascending; replace only on strictly larger
  // ts, or equal ts with strictly larger output (the holder, from an
  // earlier file, wins full ties).
  const winners = new Map<
    string,
    { rec: Rec; stem: string; isSubagent: boolean }
  >();
  for (const path of files) {
    const name = path.slice(path.lastIndexOf("/") + 1);
    const stem = name.slice(0, -".jsonl".length);
    const isSubagent = parentSessionIdFromPath(path) !== null;
    for (const [key, rec] of parseFile(path)) {
      const held = winners.get(key);
      if (
        !held ||
        rec.ts > held.rec.ts ||
        (rec.ts === held.rec.ts && rec.output > held.rec.output)
      ) {
        winners.set(key, { rec, stem, isSubagent });
      }
    }
  }
  // Group winners by (isSubagent, stem, cwd). Each subagent file becomes
  // its own session timeline so cache analysis treats each prefix stream
  // independently.
  const grouped = new Map<string, Session>();
  for (const { rec, stem, isSubagent } of winners.values()) {
    const gkey = `${isSubagent ? "S" : "P"}\u0000${stem}\u0000${rec.cwd}`;
    let session = grouped.get(gkey);
    if (!session) {
      session = {
        sessionId: stem,
        projectKey: rec.cwd,
        firstTs: rec.ts,
        lastTs: rec.ts,
        requests: [],
        byModelInputSide: new Map(),
      };
      grouped.set(gkey, session);
    }
    session.requests.push(rec);
    session.firstTs = Math.min(session.firstTs, rec.ts);
    session.lastTs = Math.max(session.lastTs, rec.ts);
    const inputSide = rec.input + rec.read + rec.w5 + rec.w1;
    session.byModelInputSide.set(
      rec.model,
      (session.byModelInputSide.get(rec.model) ?? 0) + inputSide,
    );
  }
  const sessions = [...grouped.values()];
  for (const s of sessions) {
    // Tuple sort: (ts, input, read, w5, w1) ascending.
    s.requests.sort(
      (a, b) => a.ts - b.ts || a.input - b.input || a.read - b.read || a.w5 - b.w5 || a.w1 - b.w1,
    );
  }
  // Analysis order: (projectKey, firstTs, lastTs, sessionId) ascending.
  sessions.sort((a, b) => {
    if (a.projectKey !== b.projectKey) return a.projectKey < b.projectKey ? -1 : 1;
    if (a.firstTs !== b.firstTs) return a.firstTs - b.firstTs;
    if (a.lastTs !== b.lastTs) return a.lastTs - b.lastTs;
    if (a.sessionId !== b.sessionId) return a.sessionId < b.sessionId ? -1 : 1;
    return 0;
  });
  return sessions;
}

// ---------------------------------------------------------------------------
// Analysis (the spec, re-derived)
// ---------------------------------------------------------------------------

const TTL_5M = 300;
const TTL_1H = 3600;
const THRESHOLD = 0.5;

function ratio(n: number, d: number): number {
  return d === 0 ? 0 : n / d;
}

/** First request always writes; re-write when a gap STRICTLY exceeds the
 * TTL (reads refresh the TTL, so consecutive-gap comparison is exact). */
function simulatedWriteEvents(sortedTs: number[], ttl: number): number {
  if (sortedTs.length === 0) return 0;
  let events = 1;
  for (let i = 1; i < sortedTs.length; i++) {
    if (sortedTs[i] - sortedTs[i - 1] > ttl) events += 1;
  }
  return events;
}

function dominantModel(s: Session): string {
  let best = "";
  let bestVolume = -1;
  for (const model of [...s.byModelInputSide.keys()].sort()) {
    const volume = s.byModelInputSide.get(model)!;
    if (volume > bestVolume) {
      best = model;
      bestVolume = volume;
    }
  }
  return best;
}

function analyze(sessions: Session[]) {
  // Hit rate, overall and per day.
  let read = 0;
  let inputSide = 0;
  const byDay = new Map<string, { read: number; inputSide: number }>();
  for (const s of sessions) {
    for (const r of s.requests) {
      const ris = r.input + r.read + r.w5 + r.w1;
      read += r.read;
      inputSide += ris;
      const day = utcDayFromSecs(r.ts);
      const d = byDay.get(day) ?? { read: 0, inputSide: 0 };
      d.read += r.read;
      d.inputSide += ris;
      byDay.set(day, d);
    }
  }
  const daysBelow = [...byDay.entries()]
    .sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0))
    .filter(([, d]) => d.inputSide > 0)
    .map(([day, d]) => ({ day, rate: ratio(d.read, d.inputSide), input_side_tokens: d.inputSide }))
    .filter((d) => d.rate < THRESHOLD);

  // Churn.
  let totalWrites = 0;
  let afterFirst = 0;
  let sessionsWithWrites = 0;
  const offenders: {
    session_id: string;
    first_ts: number;
    write_tokens: number;
    writes_after_first_tokens: number;
    share: number;
  }[] = [];
  // TTL counterfactual.
  let observed5m = 0;
  let observed1h = 0;
  let simulated = 0;
  let excluded = 0;
  let w5Events = 0;
  let w1Events = 0;
  let w5Tokens = 0;
  let w1Tokens = 0;
  const byModelSim = new Map<string, { w5: number; w1: number }>();
  // Cadence.
  let intraGaps = 0;
  let intraOver5m = 0;
  let zeroRead = 0;

  for (const s of sessions) {
    const writes = s.requests.map((r) => r.w5 + r.w1);
    const total = writes.reduce((a, b) => a + b, 0);
    const first = writes.find((w) => w > 0) ?? 0;
    for (const r of s.requests) {
      observed5m += r.w5;
      observed1h += r.w1;
    }
    for (let i = 1; i < s.requests.length; i++) {
      intraGaps += 1;
      if (s.requests[i].ts - s.requests[i - 1].ts > TTL_5M) intraOver5m += 1;
    }
    if (s.requests.length > 0 && s.requests.every((r) => r.read === 0)) zeroRead += 1;
    if (total === 0) {
      if (s.requests.length > 0) excluded += 1;
      continue;
    }
    sessionsWithWrites += 1;
    totalWrites += total;
    afterFirst += total - first;
    if (total - first > 0) {
      offenders.push({
        session_id: s.sessionId,
        first_ts: s.firstTs,
        write_tokens: total,
        writes_after_first_tokens: total - first,
        share: ratio(total - first, total),
      });
    }
    simulated += 1;
    const ts = s.requests.map((r) => r.ts);
    const sw5 = simulatedWriteEvents(ts, TTL_5M);
    const sw1 = simulatedWriteEvents(ts, TTL_1H);
    w5Events += sw5;
    w1Events += sw1;
    w5Tokens += sw5 * first;
    w1Tokens += sw1 * first;
    const model = dominantModel(s);
    const entry = byModelSim.get(model) ?? { w5: 0, w1: 0 };
    entry.w5 += sw5 * first;
    entry.w1 += sw1 * first;
    byModelSim.set(model, entry);
  }

  offenders.sort(
    (a, b) =>
      b.writes_after_first_tokens - a.writes_after_first_tokens ||
      a.first_ts - b.first_ts ||
      (a.session_id < b.session_id ? -1 : a.session_id > b.session_id ? 1 : 0),
  );
  const worst = offenders.slice(0, 5);

  // Dollars: per-model marginal rates, summed in sorted model-id order.
  let usd5m = 0;
  let usd1h = 0;
  let pricedTokens = 0;
  let unpricedTokens = 0;
  const unpricedModels: string[] = [];
  for (const model of [...byModelSim.keys()].sort()) {
    const sim = byModelSim.get(model)!;
    const rates = cacheRates(model);
    if (rates) {
      usd5m += (sim.w5 * (rates.cache_write_5m - rates.cache_read)) / 1e6;
      usd1h += (sim.w1 * (rates.cache_write_1h - rates.cache_read)) / 1e6;
      pricedTokens += sim.w5 + sim.w1;
    } else {
      unpricedTokens += sim.w5 + sim.w1;
      unpricedModels.push(model);
    }
  }

  // Inter-session gaps, per project (sessions sorted that way already).
  let interGaps = 0;
  let interUnder1h = 0;
  for (let i = 1; i < sessions.length; i++) {
    const prev = sessions[i - 1];
    const next = sessions[i];
    if (prev.projectKey !== next.projectKey) continue;
    interGaps += 1;
    if (Math.max(0, next.firstTs - prev.lastTs) < TTL_1H) interUnder1h += 1;
  }

  const requests = sessions.reduce((a, s) => a + s.requests.length, 0);
  return {
    sessions_analyzed: sessions.length,
    requests_analyzed: requests,
    hit_rate: {
      label: "ground truth",
      rate: ratio(read, inputSide),
      cache_read_tokens: read,
      input_side_tokens: inputSide,
      threshold: THRESHOLD,
      days_below_threshold: daysBelow,
    },
    write_churn: {
      label: "ground truth",
      total_write_tokens: totalWrites,
      writes_after_first_tokens: afterFirst,
      share: ratio(afterFirst, totalWrites),
      sessions_with_writes: sessionsWithWrites,
      worst_sessions: worst,
    },
    ttl_counterfactual: {
      label: "advisory estimate",
      inputs_label: "ground truth",
      observed_write_tokens_5m: observed5m,
      observed_write_tokens_1h: observed1h,
      sessions_simulated: simulated,
      sessions_without_writes_excluded: excluded,
      simulated_w5_write_events: w5Events,
      simulated_w1_write_events: w1Events,
      simulated_w5_write_tokens: w5Tokens,
      simulated_w1_write_tokens: w1Tokens,
      marginal_multiplier_5m: 1.15,
      marginal_multiplier_1h: 1.9,
      usd_5m_strategy: usd5m,
      usd_1h_strategy: usd1h,
      usd_delta_1h_minus_5m: usd1h - usd5m,
      priced_share_of_simulated_tokens: ratio(pricedTokens, pricedTokens + unpricedTokens),
      unpriced_models: unpricedModels,
    },
    cadence: {
      label: "ground truth",
      intra_session_gaps: intraGaps,
      intra_session_gaps_over_5m: intraOver5m,
      share_intra_gaps_over_5m: ratio(intraOver5m, intraGaps),
      inter_session_gaps: interGaps,
      inter_session_gaps_under_1h: interUnder1h,
      share_inter_gaps_under_1h: ratio(interUnder1h, interGaps),
      sessions_zero_cache_read: zeroRead,
    },
  };
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

const homeFlag = process.argv.indexOf("--home");
const home = homeFlag >= 0 && process.argv[homeFlag + 1] ? process.argv[homeFlag + 1] : homedir();
const sessions = buildSessions(home);
console.log(JSON.stringify(analyze(sessions), null, 2));
