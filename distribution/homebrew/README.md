# homebrew-tolkin

Homebrew tap for [tolkin](https://github.com/agnelnieves/tolkin), a privacy-first AI
token analyzer for agent workflows.

## Install

Tap the repository once, then install:

```sh
brew tap agnelnieves/tolkin
brew install tolkin
```

Or install in a single command without a prior tap:

```sh
brew install agnelnieves/tolkin/tolkin
```

Recent Homebrew versions gate third-party taps behind a one-time trust
decision: interactive shells prompt for it during install; scripts and CI
must run `brew trust agnelnieves/tolkin` once before `brew install`.

## Support matrix

| Platform | Architecture | Status |
| :--- | :--- | :--- |
| macOS | arm64 (Apple Silicon) | Supported |
| macOS | x64 (Intel) | Supported |
| Linux | x64 | Supported |
| Linux | arm64 | Supported |
| Windows | x64 | Use `npx @tolkin/cli` instead |

Pre-built binaries are hosted on GitHub Releases at
https://github.com/agnelnieves/tolkin. Bottles and source builds from
homebrew-core are out of scope until the project reaches the OSS extraction
milestone. This tap provides direct binary installs only.
