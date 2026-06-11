// BYOK extraction-QA scoring harness for the lossy track.
//
// What it does:
//   Each lossy fixture has a sibling fixtures/lossy/scoring/<id>.qa.json file
//   declaring N verifiable extraction questions with accepted_answers (the
//   set of strings a correct answer must match after normalization). For
//   every (fixture, target ratio) pair we send the compressed text plus each
//   question to Claude, normalize the model's answer, and check whether any
//   accepted_answers string is a substring of the normalized answer. The
//   per-case accuracy is correct/total; the track-level scored flag goes
//   true when at least one case ran.
//
// Toggle (one-command recipe; documented in methodology.md):
//   ANTHROPIC_API_KEY=<your key> bun apps/tolkin-cli/benchmarks/run.ts --score-quality
//
// Defaults: model=claude-opus-4-7 (overridable via TOLKIN_BENCH_SCORE_MODEL),
// max_tokens=128, thinking=disabled, no temperature/top_p (the model defaults).
// Cost ceiling: at most CASES * RATIOS * questions * 2 turns of input bytes
// proportional to the fixture (a few thousand tokens per call), bounded by
// the harness itself.
//
// What this is NOT: an LLM-as-judge scoring scheme. The grading is purely
// deterministic string match against author-declared accepted_answers; the
// LLM call only generates the answer. That keeps the scored numbers
// reproducible across runs in the rare event the model's wording shifts (a
// new answer string the author would have accepted but did not list is
// transparent: it shows as a wrong answer and the author can extend
// accepted_answers without rerunning the LLM call).

import { readdir, readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { REPO_ROOT } from "./cli";
import type { LossyCase, LossyQualityScoring, LossyScoredAnswer, LossyScoredCase } from "./types";

const HERE = dirname(fileURLToPath(import.meta.url));
const SCORING_DIR = resolve(HERE, "..", "fixtures", "lossy", "scoring");

const ANTHROPIC_API_URL = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION = "2023-06-01";
// Pinned default model; the env var override is documented in methodology.md.
// Defaulting to claude-opus-4-7 keeps the scored row's intelligence ceiling
// the highest available; cheaper models can score the same fixtures with a
// one-flag swap but the scored=true row's baseline is always the same model
// across reruns unless the operator changes it.
const DEFAULT_MODEL = "claude-opus-4-7";
const MAX_TOKENS = 128;

export interface ScoreOptions {
  enabled: boolean;
  reason: string;
}

interface QaFixture {
  fixture_id: string;
  fixture_path: string;
  note: string;
  questions: Array<{
    id: string;
    question: string;
    accepted_answers: string[];
  }>;
}

/**
 * Decide whether to run scoring based on env. The single-flag contract is:
 * --score-quality must be passed (toplevel run.ts plumbs it into the bool
 * here) AND ANTHROPIC_API_KEY must be set. Either missing keeps scored=false
 * with a precise reason; the run never silently silently picks one or the
 * other.
 */
export function decideScoring(flagPassed: boolean): ScoreOptions {
  if (!flagPassed) {
    return {
      enabled: false,
      reason: "--score-quality flag not passed (default off)",
    };
  }
  const key = process.env.ANTHROPIC_API_KEY;
  if (key === undefined || key.length === 0) {
    return {
      enabled: false,
      reason:
        "ANTHROPIC_API_KEY not set; --score-quality requires the operator's own key (BYOK contract)",
    };
  }
  return { enabled: true, reason: "ANTHROPIC_API_KEY present and --score-quality passed" };
}

/**
 * Discover the per-fixture QA JSON files. The harness errors loudly when a
 * lossy case has no matching scoring fixture so a row cannot ship scored=true
 * with an undeclared denominator (per the track contract).
 */
async function loadAllFixtures(): Promise<Map<string, QaFixture>> {
  const map = new Map<string, QaFixture>();
  let entries: string[];
  try {
    entries = await readdir(SCORING_DIR);
  } catch {
    return map;
  }
  for (const name of entries) {
    if (!name.endsWith(".qa.json")) continue;
    const fullPath = resolve(SCORING_DIR, name);
    const text = await readFile(fullPath, "utf8");
    const parsed = JSON.parse(text) as QaFixture;
    if (typeof parsed.fixture_id !== "string") {
      throw new Error(`scoring fixture ${name} is missing fixture_id`);
    }
    if (!Array.isArray(parsed.questions) || parsed.questions.length === 0) {
      throw new Error(`scoring fixture ${name} has no questions`);
    }
    map.set(parsed.fixture_id, parsed);
  }
  return map;
}

/**
 * Lowercase, collapse all internal whitespace, strip trailing punctuation,
 * and trim. The transform applies to BOTH sides of the match (model output
 * and accepted_answers) so the comparison is symmetric.
 */
function normalize(s: string): string {
  return s
    .toLowerCase()
    .replace(/[\s ]+/g, " ")
    .replace(/^[.,;:!?'"`)\]\s]+|[.,;:!?'"`(\[\s]+$/g, "")
    .trim();
}

function isCorrect(answer: string, accepted: string[]): boolean {
  const a = normalize(answer);
  if (a === "") return false;
  for (const candidate of accepted) {
    const c = normalize(candidate);
    if (c === "") continue;
    // Exact match OR the accepted answer appears as a substring of the model
    // answer (the model often returns a one-sentence answer with the fact as
    // a substring; an extracted-fact harness should accept that).
    if (a === c) return true;
    if (a.includes(c)) return true;
  }
  return false;
}

interface AnthropicResponse {
  content: Array<{ type: string; text?: string }>;
  stop_reason: string;
}

async function callClaude(
  model: string,
  key: string,
  context: string,
  question: string,
): Promise<string> {
  // System: extraction harness instruction set. Kept deterministic by
  // avoiding any per-run randomness (no timestamp, no fixture-specific
  // boilerplate beyond the context block itself). Adaptive thinking off:
  // these are short extraction-style answers and we explicitly want the
  // model to NOT think out loud.
  const system =
    "You are an extraction-grading agent. Answer the user's question using ONLY the information present in the provided context. Reply with the shortest exact phrase from the context that answers the question. No explanation, no leading words like 'Answer:' or 'The'. If the context does not contain the answer, reply with the literal string UNKNOWN.";
  const userContent = `Context:\n\`\`\`\n${context}\n\`\`\`\n\nQuestion: ${question}`;
  const body = {
    model,
    max_tokens: MAX_TOKENS,
    system,
    thinking: { type: "disabled" } as const,
    messages: [{ role: "user", content: userContent }],
  };
  const resp = await fetch(ANTHROPIC_API_URL, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "x-api-key": key,
      "anthropic-version": ANTHROPIC_VERSION,
    },
    body: JSON.stringify(body),
  });
  if (!resp.ok) {
    const errText = await resp.text();
    throw new Error(`Anthropic API ${resp.status}: ${errText.slice(0, 300)}`);
  }
  const parsed = (await resp.json()) as AnthropicResponse;
  const textBlock = parsed.content.find((b) => b.type === "text");
  if (!textBlock || textBlock.text === undefined) {
    throw new Error(
      `Anthropic response had no text block: ${JSON.stringify(parsed).slice(0, 300)}`,
    );
  }
  return textBlock.text.trim();
}

/**
 * Score every lossy case via Claude. The track-level quality_scoring flag is
 * returned alongside the per-case scored answers; the runner attaches the
 * answers to the matching LossyCase before publishing.
 */
export async function scoreAllCases(
  cases: LossyCase[],
  compressedByCase: Map<string, string>,
): Promise<{ qualityScoring: LossyQualityScoring; scoredByCase: Map<string, LossyScoredCase> }> {
  const key = process.env.ANTHROPIC_API_KEY;
  if (key === undefined || key.length === 0) {
    throw new Error("ANTHROPIC_API_KEY not set; scoreAllCases must not be called");
  }
  const model = process.env.TOLKIN_BENCH_SCORE_MODEL ?? DEFAULT_MODEL;
  const fixtures = await loadAllFixtures();
  if (fixtures.size === 0) {
    throw new Error(
      `no scoring fixtures found under ${SCORING_DIR}; cannot run --score-quality without QA fixtures`,
    );
  }

  // Per-case score accumulator; each lossy case ID -> {correct, total, answers}.
  const scored = new Map<string, LossyScoredCase>();
  // Group lossy cases by their fixture id (the prefix before "@rate=").
  for (const c of cases) {
    const baseId = c.id.split("@")[0] ?? c.id;
    const qa = fixtures.get(baseId);
    if (qa === undefined) {
      throw new Error(
        `scoring fixture for ${baseId} is missing; add benchmarks/fixtures/lossy/scoring/${baseId.split("/").pop()}.qa.json before running --score-quality`,
      );
    }
    const compressed = compressedByCase.get(c.id);
    if (compressed === undefined) {
      throw new Error(`runner did not capture compressed text for ${c.id}`);
    }
    const answers: LossyScoredAnswer[] = [];
    let correct = 0;
    for (const q of qa.questions) {
      const reply = await callClaude(model, key, compressed, q.question);
      const ok = isCorrect(reply, q.accepted_answers);
      if (ok) correct += 1;
      answers.push({
        question_id: q.id,
        question: q.question,
        reply,
        accepted_answers: q.accepted_answers,
        correct: ok,
      });
    }
    scored.set(c.id, {
      case_id: c.id,
      questions_total: qa.questions.length,
      questions_correct: correct,
      accuracy: qa.questions.length === 0 ? 0 : correct / qa.questions.length,
      answers,
    });
  }

  return {
    qualityScoring: {
      scored: true,
      method: `BYOK extraction-QA harness (model=${model}); per-case accuracy is exact-or-substring match of model output against author-declared accepted_answers`,
      grader_model: model,
      anthropic_version: ANTHROPIC_VERSION,
    },
    scoredByCase: scored,
  };
}

export const SCORING_DIR_ABSOLUTE = SCORING_DIR;
export const RELATIVE_SCORING_DIR = SCORING_DIR.replace(`${REPO_ROOT}/`, "");
