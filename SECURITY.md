# Security Policy

## Reporting a vulnerability

If you discover a security vulnerability in Tolkin, please report it responsibly rather than through a public issue.

**Email:** agnelnieves@gmail.com (subject line: "Security vulnerability in Tolkin")

Alternatively, use GitHub's private vulnerability reporting feature if available in your repository settings.

We will acknowledge your report within 48 hours and will work on a fix and timeline for disclosure.

## Scope

This policy covers vulnerabilities in the Tolkin codebase and official distributions (npm, Homebrew tap, GitHub releases).

## Data and privacy

Tolkin is designed with privacy as a first principle. See [PRIVACY.md](PRIVACY.md) for the full data handling posture.

Key points:

- The analyzer runs entirely on your machine. No telemetry, no uploads, no network calls by default.
- Optional BYOK tokenizer verification requires your own API key, stored only in local memory.
- The CLI ledger is local-only, consented, and fully resettable.
- Network egress is strictly limited and user-controlled.

If you believe a privacy feature is broken or if you discover a way for the tool to exfiltrate data unexpectedly, treat it as a security issue and report it via the channel above.

## Supported versions

We support the latest stable release. Patch releases for critical security issues may be backported to the previous minor version at maintainer discretion.

## License

This security policy is part of the Tolkin project and is licensed under the MIT License.
