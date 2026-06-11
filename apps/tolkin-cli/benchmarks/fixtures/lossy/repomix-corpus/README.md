# lossy/repomix corpus

Self-authored declared corpus for the repomix `--compress` comparison row in
the benchmark's lossy track. The repomix README claims about a 70 percent
token reduction for `--compress` against full repo packs without disclosing
a basis (no methodology, no benchmark fixture, no test set is published with
the claim). The honest move is to run repomix headlessly on a small,
declared corpus and publish what the harness measures alongside the claimed
figure with both denominators stated.

## Provenance

Each file in this directory is written by the repository owner for this
benchmark. Nothing is copied from a third-party project. The fixture
deliberately mixes TypeScript (function-heavy, the case repomix `--compress`
optimizes for) with Python (different tree-sitter parser path) so the
comparison exercises the compressor on more than one language.

Files:

- `auth.ts`: TypeScript module with interfaces, a JSDoc-decorated public
  function, and five helper function bodies.
- `billing.ts`: TypeScript module with an `Invoice` interface and four
  public functions invoking the Stripe SDK.
- `email.py`: Python module with a dataclass, a class with private and
  public methods, and a template renderer.

License: MIT (per the repository root).

## How the row is measured

The runner shells out to the pinned `repomix@1.14.1` binary (installed as a
dev dependency of `apps/tolkin-cli`; see `package.json`) twice against this
directory:

1. `repomix <corpus> -o <out>`: the baseline pack without `--compress`.
2. `repomix --compress <corpus> -o <out>`: the same pack with `--compress`
   enabled (tree-sitter strips function bodies and keeps signatures).

Both outputs are tokenized through `tolkin count --model openai` (o200k_base,
exact). The achieved saved percent is published next to the upstream claim
of ~70 percent. The basis for the upstream claim is undisclosed and the
benchmark says so in the comparison row's `reason` field.
