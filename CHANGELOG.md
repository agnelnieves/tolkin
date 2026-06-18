# Changelog

All notable changes to this project are documented in this file.

The format is loosely based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Extracted Tolkin from the `agnelnieves/agnelweb` monorepo into this standalone repository.
- Migrated npm distribution from unscoped `tolkin-cli` / `tolkin-<platform>-<arch>` to the `@tolkin` scope. New package names: `@tolkin/cli`, `@tolkin/darwin-arm64`, `@tolkin/darwin-x64`, `@tolkin/linux-arm64`, `@tolkin/linux-x64`, `@tolkin/win32-x64`.
- Internal WASM package renamed from `tolkin-core-wasm` to `@tolkin/core-wasm` (workspace-internal, `private: true`).
- CI publish workflow updated to use `GITHUB_TOKEN` for in-repo release creation, with `HOMEBREW_TAP_TOKEN` retained for the cross-repo tap update.

### Deprecated

- The old unscoped npm packages (`tolkin-cli`, `tolkin-darwin-arm64`, etc.) will be marked deprecated on npm once `@tolkin/*` is published. Existing lockfiles will continue to resolve.

## [0.15.1] - 2026-06-11

For releases at or before 0.15.1, see the git history in this repository. Older versions were released from the `agnelnieves/agnelweb` monorepo before extraction.
