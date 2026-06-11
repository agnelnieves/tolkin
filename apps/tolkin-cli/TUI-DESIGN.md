# Tolkin TUI 2.0: design and implementation contract

Date: 2026-06-11. Status: approved for implementation on branch `feat/tolkin-tui`.

This document is the single source of truth for the dashboard revamp. Implementation
agents read this top to bottom before touching code. Where this doc and existing code
disagree, this doc wins, except for the Hard Contracts section, which always wins.

## 0. Goals

1. **Beautiful.** The dashboard should feel designed, not rendered: layered surfaces,
   intentional spacing, a real theme system, purposeful micro-animation.
2. **Useful.** Selection, drill-down detail, in-dashboard actions (rescan, audit a file,
   copy values, generate the HTML report), a command palette, discoverable keys.
3. **Great for agents too.** Every TUI surface keeps a non-TTY equivalent. The compact
   frame and the JSON contracts stay first-class. The help overlay teaches agents the
   CLI equivalents.
4. **Honest.** Tier labels (identified / realized / measured) and the honesty line render
   on every numeric surface, exactly as today.

Benchmark: OpenCode's TUI (surface layering, toasts with colored side rails, command
palette with inline keybindings, scrim-dimmed dialogs, animation kill switch) and
lazygit (panel-local keys plus a `?` overlay). We replicate the outcomes in ratatui.

## 1. Hard contracts (breaking any of these fails review)

- `tui::run()` and `tui::render_compact_frame()` keep their signatures. `stats --tui`,
  `stats --compact`, and bare `tolkin` dispatch exactly as today.
- `tui::data` stays `pub`: `commands::report` imports it (`use crate::tui::data`).
- `tests/tui.rs` behavior contracts hold: bare piped `tolkin` exits 2 with usage on
  stderr; `stats --tui` under non-TTY fails fast mentioning the terminal; the compact
  frame contains `tolkin`, `Project`, `Machine`, `Spend`, and the honesty line; a fresh
  data dir renders the setup card. Tests may gain assertions, never lose them.
- `HONESTY_LINE` text is owner-sacred: `input savings, output may vary`. Render it
  verbatim in the footer and the compact frame.
- The existing `mod.rs` test expecting the string `advisory` in a static frame must
  stay satisfiable (tier tags on cards cover it).
- **Zero new dependencies.** No new crates. Easing is arithmetic, base64 for OSC52 is
  20 lines, color blending is arithmetic. cargo-deny and the size budget stay untouched.
- ratatui stays at 0.29, crossterm at 0.28. Sync event loop: std threads plus mpsc, no
  tokio.
- Panic hook terminal restore stays installed and idempotent (current `install_panic_hook`).
- `cargo test --all-targets`, `cargo clippy --all-targets -- -D warnings` (CI clippy is
  NEWER than local: write ?-idiomatic, collapsible-if-clean code), `cargo fmt --check`
  all green at every checkpoint commit.
- No em-dashes or en-dashes in any prose, comment, or doc line. Use periods, commas,
  colons, parentheses.
- Never log work into basestream. Never add Co-Authored-By lines to commits.

## 2. Module map (target layout)

```
src/tui/
  mod.rs          public seam: run(), render_compact_frame(), pub mod data; wiring only
  app.rs          Model, Msg, update(), route + overlay stack, Cmd dispatch
  event.rs        input thread (crossterm events -> Msg), worker spawns (scan, reload)
  theme.rs        Theme struct, 12-step scales, built-ins, detection, NO_COLOR
  anim.rs         Animator, Tween, easing, Clock trait (ManualClock for tests)
  keymap.rs       Action enum, Context, the binding table (single source of truth)
  format.rs       commas, usd, token humanize, truncate_left, relative time, base64
  components/
    mod.rs
    chrome.rs     header (logo block, tab strip, status cluster), footer (hints + honesty)
    card.rs       StatCard: big value, label, tier tag, optional delta and mini-spark
    list.rs       SelectList: selection rail, scrollbar, vim nav, optional filter line
    table.rs      DataTable: header, sort indicator, selection
    bars.rs       HBarRow: label, animated fill, value; BarStrip for the 30-day chart
    gauge.rs      slim percent gauge (cache hit)
    modal.rs      centered dialog: 3 widths (60/88/116), scrim dim, title, esc close
    palette.rs    command palette: fuzzy filter over Actions, keybinding rendered inline
    help.rs       grouped keymap overlay, generated from keymap.rs, plus agents section
    toast.rs      ToastStack: 4 variants, colored side rails, ttl, max 3
    spinner.rs    braille spinner frames + scanner label (gradient sweep with trail)
    empty.rs      empty and zero states; setup card is the brand moment
  screens/
    mod.rs        Screen routing helpers
    overview.rs   NEW default tab
    project.rs    load profile, heavy files (selectable), savings
    machine.rs    totals cards, ranked projects (selectable), project detail data
    spend.rs      30-day bars (selectable day), models table, cache, advisories
  data.rs         kept and extended: pure helpers only, no IO, no clock reads
```

`ui.rs` is deleted at the end of wave 1 (its logic redistributed). `data.rs` keeps its
public API and grows new pure helpers.

## 3. Runtime architecture

Elm-shaped, synchronous:

- **One input thread** reads `crossterm::event::read()` forever, sends `Msg::Key`,
  `Msg::Mouse`, `Msg::Resize` over an `mpsc::Sender<Msg>`.
- **Worker threads** per task send `Msg::ScanDone(Result<ProjectReport, String>)`,
  `Msg::SnapshotLoaded(Box<StatsSnapshot>)`, `Msg::AuditDone { path, result }`,
  `Msg::ReportDone(Result<PathBuf, String>)`.
- **Main loop**: `rx.recv_timeout(next_deadline)` where `next_deadline` is:
  - 33 ms while `animator.active()` or a spinner is visible (30 fps),
  - 100 ms on the setup screen while the pulse runs (animations enabled only),
  - otherwise 1000 ms (data freshness tick re-renders relative times).
- `update(model, msg) -> Vec<Cmd>` mutates the model. `Cmd` covers `SpawnScan`,
  `ReloadSnapshot`, `RunAudit(path)`, `RunReport`, `CopyOsc52(String)`, `Quit`.
- `view(frame, model)` is pure. **Derived data is NOT computed in view.** A `Derived`
  struct (tier reports, machine projects, day bars, model rows, cache, advisories) is
  recomputed only on `SnapshotLoaded` and `ScanDone`. Today's code recomputes
  everything every second; at 30 fps that would be unacceptable.
- Resize: relayout happens naturally per frame. Below 80x24 render a centered
  "terminal too small" card naming `tolkin stats --json` as the fallback.

## 4. Theme system

Radix-style 12-step scale per theme, semantic tokens on top. About 30 tokens total:

```rust
pub struct Theme {
    pub name: &'static str,
    // surfaces (the layering trick: panels differ by background, not borders)
    pub bg: Color,           // step1: app background. Color::Reset in `terminal` theme
    pub surface: Color,      // step2: panel background
    pub element: Color,      // step3: cards, selected-adjacent
    pub overlay: Color,      // step4: modal and palette background, hover
    // lines
    pub border_subtle: Color, // step6
    pub border: Color,        // step7
    pub border_active: Color, // step8
    // text
    pub text: Color,          // step12
    pub muted: Color,         // step11
    pub faint: Color,         // step8-9 of the gray ramp
    // brand and semantics
    pub accent: Color,        // step9, tolkin cyan
    pub accent_bright: Color, // step10
    pub ok: Color, pub warn: Color, pub err: Color, pub info: Color,
    // derived
    pub selection_bg: Color, pub selection_fg: Color,
    pub bar_fill: Color, pub bar_empty: Color,
    pub scrim_factor: f32,    // 0.45: multiply RGB under modals
}
```

Built-ins:
- `tolkin-dark` (default): scale anchored on `#0A0E12 -> #E6EDF3`, accent `#22D3EE`
  (cyan, matches the landing), ok `#4ADE80`, warn `#FBBF24`, err `#F87171`,
  info `#60A5FA`.
- `tolkin-light`: inverted scale, same accents darkened one step.
- `terminal`: bg and surface are `Color::Reset` (respect the user's terminal background,
  OpenCode's `system` idea), text colors from the 16-color set.
- `mono`: no color at all beyond Reset, White, Gray, DarkGray. Forced when `NO_COLOR`
  is set or the theme detection finds a dumb terminal.

Detection: `COLORTERM` containing `truecolor` or `24bit` selects RGB values; otherwise
each RGB token degrades to a precomputed 256-color index (store both in the scale
definition, a `const` table, computed by hand once, no runtime quantizer needed).
Selection precedence: `TOLKIN_THEME` env, then `[ui] theme` in config.toml, then
default. `t` cycles themes at runtime; persisting the choice writes the existing
config through the existing save path (additive serde field, see section 9).
`selection_fg` is computed contrast-safe: light themes get step12-dark, dark themes
step1, the OpenCode `selectedForeground` idea.

Style rule: **no `Color::` literal anywhere outside theme.rs.** Screens and components
take `&Theme`.

## 5. Animation engine

```rust
pub trait Clock { fn now(&self) -> Instant; }            // SystemClock, ManualClock
pub enum Ease { Linear, OutCubic, InOutCubic }
pub struct Animator { /* HashMap<AnimKey, Tween>, clock, enabled: bool */ }
```

- `go(key, to, dur, ease)`: retargeting an active tween starts from its CURRENT value
  (smooth interruption). `value(key, fallback)` samples. `active()` says whether any
  tween is unfinished (drives the 33 ms cadence). Finished tweens are pruned.
- `enabled = false` (reduced motion) makes `go` snap instantly and `active()` return
  false. Set from `TOLKIN_REDUCED_MOTION=1`, `NO_COLOR` does not imply it.
- All consumers sample through the animator; nothing calls `Instant::now()` directly
  except the SystemClock. Tests use ManualClock and step time explicitly.

Animation inventory (timings fixed here so waves do not bikeshed):

| Surface | Effect | Spec |
|---|---|---|
| Braille spinner | scan and load busy states | frames `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`, 80 ms per frame; static fallback `⋯` when disabled |
| Scanner label | "scanning repo" during scan | per-char color sweep, accent head with 4-char muted trail, bidirectional with 3-frame hold at the ends, advances every frame at 30 fps |
| Bars (load profile, projects, cache gauge) | fill grows to target | 0 to target on first data, old to new on refresh, 300 ms OutCubic, per-row stagger 30 ms |
| Hero numbers (Overview cards) | count up | 400 ms OutCubic from previous value to new, formatted through the humanizer each frame |
| Tab indicator | underline slides to active tab | 150 ms OutCubic on x position |
| Toast | slide in from right edge | 150 ms OutCubic, auto-dismiss after 5000 ms, removal is instant |
| Row reveal (tables and lists on data arrival) | staggered birth | row i appears at i * 30 ms, one ramp step faint -> muted -> normal |
| Setup card pulse | brand moment on the empty state ONLY | accent border breathing, sine, 4600 ms period, amplitude mapped to a 3-step color ramp; runs at 100 ms cadence; never on data screens |

Idle discipline: when nothing animates, the loop blocks up to 1 s. Animations never
run continuously on data screens, so battery cost stays nil.

## 6. Keymap (single source of truth)

`keymap.rs` declares every action once; the footer hints, the help overlay, and the
palette all render from this table. Contexts: `Global`, `List`, `DayStrip`, `Modal`,
`PaletteInput`, `FilterInput`.

| Keys | Action | Context | Notes |
|---|---|---|---|
| `q`, `ctrl+c` | Quit | Global | `q` suppressed in input contexts |
| `esc` | Back / close overlay | Global | pops the overlay stack, then no-op (never quits) |
| `1` `2` `3` `4` | GoTab(Overview, Project, Machine, Spend) | Global | |
| `tab` / `shift+tab` | NextTab / PrevTab | Global | arrows do NOT switch tabs anymore (they navigate) |
| `j`/`down`, `k`/`up` | Down / Up | List | also panel focus targets |
| `g` / `G` | Top / Bottom | List | |
| `ctrl+d` / `ctrl+u` | Half page down / up | List | |
| `h`/`left`, `l`/`right` | Day left / right | DayStrip | Spend 30-day strip |
| `[` / `]` | Cycle panel focus | Global | screens with multiple interactive panels |
| `enter` | Open detail | List | file detail, project detail, advisory detail, day detail |
| `r` | Refresh snapshot | Global | reload ledger and usage in a worker, toast on done |
| `s` | Rescan project | Global | scan worker, scanner label while running |
| `a` | Audit selected file | Project heavy-files list | worker, opens file detail with findings |
| `o` | Generate HTML report | Global | runs the report command in a worker, toast with path |
| `y` | Copy (path or value under selection) | List | OSC52, toast "copied" |
| `/` | Filter list | List | inline filter line, esc clears |
| `,` | Cycle sort | List, Table | machine projects, spend models |
| `t` | Cycle theme | Global | persists via config |
| `?` | Help overlay | Global | |
| `ctrl+k`, `:` | Command palette | Global | |

Keymap tests: no duplicate binding within a context; every `Action` is bound or
palette-only; help renders every group.

Mouse: wheel scrolls the focused list (3 lines per notch), click selects a row,
click on a tab switches, click outside a modal closes it. Mouse capture is already
enabled today; this wires it.

## 7. Screens

### Chrome (every screen)

```
 tolkin  Overview  Project  Machine  Spend            ● ingestion  data 2m  v0.15
 ───────────────────────────────────────────────────────────────────────────────
 [body]
 j/k navigate   enter detail   s scan   ? help                  ctrl+k commands
 input savings, output may vary. tiers: identified (advisory) ... prices 2026-06-10
```

- ` tolkin ` is a logo block: accent background, step1 text, one cell of padding.
- Active tab: text step12 plus animated accent underline; inactive: muted.
- Right cluster: ingestion dot (`●` ok-colored on, `○` muted off), snapshot age,
  version, and an update chip (`update 0.16` info-colored) only if the existing
  passive update cache already knows a newer version (read-only seam, no network).
- Footer row 1: contextual hints from the keymap (top 4 for the focused context),
  right-aligned palette hint. Row 2: the honesty line verbatim plus tier legend and
  prices date, exactly today's text.
- Toasts overlay top-right at y 2, max width 60, variant-colored `┃` side rails on
  the overlay surface, bold first word, max 3 stacked.

### Overview (new, default tab)

```
 ┌ today ─────────┐ ┌ 30 days ───────┐ ┌ cache hit ─────┐ ┌ reclaimable ───────┐
 │ $4.12          │ │ $61.55         │ │ 81.3%  ▰▰▰▰▰▱▱ │ │ 12.4k - 48.1k tok  │
 │ measured       │ │ measured       │ │ measured       │ │ advisory           │
 └────────────────┘ └────────────────┘ └────────────────┘ └────────────────────┘
 ┌ spend, last 30 days ──────────────────────────────────────── max $9.80 ┐
 │  ▂▃▂▅▆▃▂▁▂▃▅█▆▅▃▂▁▁▂▃▄▅▆▅▄▃▂▂▃▄                                        │
 └─────────────────────────────────────────────────────────────────────────┘
 ┌ advisories (j/k, enter) ───────────────┐ ┌ this machine ────────────────┐
 │ ▸ output share 15.3% of bill ($9.41)   │ │ projects tracked         14  │
 │   cap runway: $200 reached ~Jun 24     │ │ sessions ingested       481  │
 │   opus-4-7 at 94% of priced spend      │ │ last scan      2m ago (1.2s) │
 └────────────────────────────────────────┘ └──────────────────────────────┘
```

- Cards: count-up numbers, tier tag under each value (measured / advisory), the
  cache card embeds the slim gauge. Cards with no data show a muted hint
  ("consent to ingestion: tolkin init").
- Spend spark: day bars, today's bar accent-bright, max label in the title.
- Advisories list is selectable; enter opens the advisory detail modal.
- When ingestion is off, the spend and cache cards plus the advisories panel show
  their consent empty states; the layout never collapses.

### Project

- Load profile: 4 animated HBars with share percent and token count, source line
  ("live scan, 312 files in 1.2 s" or scanner label while running).
- Heavy files: SelectList, directory part muted, filename bright, right-aligned
  tokens plus percent of always-loaded. Scrollbar when overflowing. `a` audits the
  selected file in a worker; `enter` opens file detail (tokens, percent of profile,
  audit findings when ready, suggested next actions); `y` copies the path.
- Savings: identified and realized lines with tier tags, realized sparkline with
  the baseline note.

### Machine

- Totals row: identified range, realized total, projects, sessions (StatCards).
- Projects SelectList: name, animated weight bar, tokens, sessions, relative
  "as of" timestamp (this brings `MachineProject::last_ts` alive; remove its
  `#[allow(dead_code)]`). Sort cycle `,`: weight, sessions, recency.
- `enter` opens project detail: snapshots-over-time sparkline, first and last
  seen, sessions, and the hint to cd there and rerun `tolkin project`.

### Spend

- 30-day BarStrip with a selectable day cursor (`h`/`l`): the selected day's exact
  numbers render in a caption line (date, input incl cache, fresh input, output,
  cost). Today is accent-bright.
- Models DataTable: top 5 by input volume plus a "+N more" muted footer row,
  sort cycle on `,` (input, output, cost). Unpriced costs render muted.
- Cache panel: hit-rate gauge, the health line (threshold wording unchanged from
  today), one TTL economics headline from CacheReport when present.
- Advisories SelectList, enter for detail modal (full text plus the levers
  sentence from the existing advisory copy).
- Ingestion off: one friendly full-panel empty state naming the exact consent
  command, never three separate stubs.

### Modals

Dialog widths 60, 88, 116 (medium, large, xlarge), clamped to width minus 2; top
at height/4. The scrim dims every underlying cell: RGB colors multiply by
`scrim_factor`, indexed and named colors map to their dim ramp neighbor (a small
const lookup), `mono` theme skips dimming and just clears. Esc closes, overlays
stack (help over a detail modal works). Help overlay (`?`): keymap groups
(Navigate, Act, View) generated from keymap.rs plus an **Agents** section listing
the CLI equivalents (`tolkin stats --json --global`, `tolkin stats --compact`,
`tolkin project --json`, `tolkin mcp --json`, exit codes, `NO_COLOR`,
`TOLKIN_REDUCED_MOTION`, `TOLKIN_THEME`). Palette (`ctrl+k` or `:`): fuzzy filter
(subsequence match with simple scoring: consecutive runs and word starts weigh
more; 30 lines of code, no dep), every action listed with its keybinding rendered
muted on the right, suggested actions float first.

### Setup (fresh data dir)

The brand moment: tolkin wordmark (block glyphs), the breathing accent border,
the three getting-started commands, the privacy line, `q to quit`. This is the
ONLY screen with an idle animation.

## 8. Compact frame and agent parity

`render_compact_frame()` renders the Overview tab at 100x30 through the same
view code with `Animator::disabled()` and bars at full target (deterministic,
byte-stable for a given snapshot). It must contain `tolkin`, all four tab titles
(satisfies the existing Project, Machine, Spend assertions), a tier tag
(`advisory` or `measured`), and the honesty line. Setup state renders the setup
card as today. tests/tui.rs gains an `Overview` assertion, additively.

## 9. Config and env

- `Config` (ledger.rs) gains one additive field, following the existing optional
  pattern exactly: `#[serde(skip_serializing_if = "Option::is_none", default)] pub ui_theme: Option<String>`.
  Round-trips cleanly with older binaries. Saved through the existing save path
  when the user cycles themes (only when a config already exists; never create a
  config from the TUI, consent stays init's job).
- Env: `TOLKIN_THEME` (overrides config), `TOLKIN_REDUCED_MOTION=1`, `NO_COLOR`
  (forces mono), existing `TOLKIN_DATA_DIR` untouched.

## 10. Performance budget

- Derived data computed on data messages only, never in view.
- No file IO, no env reads, no clock syscalls inside view (theme and animator are
  inputs). Snapshot reload and scans happen in workers with busy states.
- 30 fps only while animating; idle blocks on recv up to 1 s.
- Per-frame allocations: Lines and Spans are unavoidable in ratatui; everything
  else (derived rows, sorted vectors) is cached on the model.

## 11. Testing plan

- `anim.rs`: ManualClock tween math, retarget-from-current, prune, reduced motion.
- `theme.rs`: NO_COLOR forces mono, TOLKIN_THEME selection, 256-color degradation
  (env injected through parameters, no global env mutation in unit tests).
- `keymap.rs`: uniqueness per context, full coverage, help generation.
- `format.rs`: humanize, relative time, base64 vectors.
- Screens: TestBackend frames per state (empty, loading, ready, error) asserting
  load-bearing strings and selection movement, animator disabled.
- `app.rs`: update() unit tests for tab nav, overlay stack push and pop, palette
  execution, toast lifecycle on a ManualClock, refresh and scan flows with
  synthetic worker messages.
- tests/tui.rs: existing four tests stay green; add Overview to the compact
  asserts; add a non-TTY guard test for any new flag only if a new flag appears.
- The full preexisting suite (403 tests) stays green.

## 12. Wave plan

Wave 1, framework and parity (this is the big one):
theme.rs, anim.rs, keymap.rs, format.rs, event.rs, app.rs, components (chrome,
card, list, bars, gauge, spinner, empty, modal shell), screens at functional
parity plus Overview, selection on every list, compact frame on the new chrome,
ui.rs deleted, all gates green. Checkpoint commit after each compiling cluster:
(1) theme+anim+keymap+format with tests, (2) components, (3) app+event+screens,
(4) compact frame + test updates.

Wave 2, interactivity and tools:
detail modals (file with worker audit, project, advisory, day caption), palette,
help overlay, toasts wired to refresh/scan/copy/report/errors, OSC52 copy,
sort cycles, list filter, mouse wiring, update chip read-only seam, config
ui_theme field, README "For agents" section.

Wave 3, motion and micro-polish:
the full animation inventory wired and timed per section 5, setup brand moment,
staggered reveals, scanner label, scrim refinement, reduced-motion and NO_COLOR
audits, minimum-size guard, spacing and empty-state copy pass, compact frame
restyle, tests/tui.rs additions, doc touches.

Wave 4, adversarial review (read-only, three lenses: terminal safety and panic
paths, performance and allocation discipline, contract and clippy strictness),
then one fix wave.

Wave 5, orchestrator: full gates, PTY smoke pass, version bump 0.14.0 to 0.15.0,
PROGRESS.md entry, final report.

## 13. Engagement mechanics for implementation agents

- Work in `/Users/agnel/Documents/agnel-website-tui/apps/tolkin-cli` on branch
  `feat/tolkin-tui` only. Anchor every git command with
  `git -C /Users/agnel/Documents/agnel-website-tui` and verify
  `git branch --show-current` prints `feat/tolkin-tui` before committing.
- Checkpoint commits as you go (the previous engagement lost an agent that tried
  to emit everything at the end). Conventional style, tolkin scope, for example
  `feat(tolkin): tui theme system and animation engine`.
- Keep final messages SHORT: a list of commits, gate results, and any deviations
  from this doc. Do not paste code into the final message.
- Run gates from `apps/tolkin-cli`: `cargo test --all-targets`,
  `cargo clippy --all-targets -- -D warnings`, `cargo fmt -- --check`.
- If a contract in section 1 forces a design change, stop that piece, note it in
  the final message, do not improvise around a contract.

## 14. As built (deviations from the sections above)

Waves 1 and 2:
- Spend day captions show pro-rata `~` costs (cross-day sessions split evenly).
- The cache panel drops the standalone total line (gauge, health, TTL verdict).
- Advisory detail "levers" paragraphs reuse the model-mix sentences verbatim.
- `,` also binds in DayStrip (the Spend models table is not focusable).
- Modal height is body + 2 border rows, clamped to the frame, not fixed sizes.

Wave 3:
- Reveal and weight tweens key on row identity (FNV-1a of path / project key /
  line text); refreshes ramp only genuinely new rows; Overview and Spend
  advisories share one reveal namespace; stagger fires for the first 64 rows.
- The scanner's 3-frame end holds count the arrival frame.
- The setup pulse maps its sine onto border_active / accent / accent_bright;
  reduced motion pins it at plain accent, and the loop idles while busy under
  reduced motion (the spinner is static, so 30 fps would only burn battery).
- Mono selection renders through REVERSED, not selection colors.
- The footer compresses the tier legend on narrow frames so the honesty
  sentence stays verbatim and the prices date stays visible.
- The compact frame suppresses selection chrome (`static_frame`): no rails,
  the day cursor renders as today's accent highlight.
- Modals scroll (j/k, arrows, wheel) when the body exceeds the dialog; the
  offset resets on stack changes; the border advertises remaining rows.
- No-data cards distinguish consent-off ("off (tolkin init)") from
  ingestion-on-but-empty ("no sessions yet").
