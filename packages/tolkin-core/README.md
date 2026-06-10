# tolkin-core

Rust core of Tolkin, a browser-based AI token analyzer that also ships as a CLI. This workspace contains the shared analysis library (`tolkin-core`) and its WebAssembly bindings (`tolkin-core-wasm`). See `apps/tolkin-web/PLAN.md` for the full project plan.

## Commands

```
cargo test
cargo clippy --all-targets -- -D warnings
bun run build
```

`bun run build` invokes `wasm-pack` to produce the `pkg/` output consumed by the Next.js app.
