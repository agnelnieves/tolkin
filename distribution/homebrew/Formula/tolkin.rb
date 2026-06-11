# This file is the staged template. The live copy lives in the tap repo
# agnelnieves/homebrew-tolkin. The homebrew-release CI job substitutes
# the version string and the four sha256 values on every release using
# sed against the placeholder tokens below. Do not hand-edit the sha256
# lines in this file; they exist only so the formula parses correctly as
# a template. The zeroed sha256 values will not pass `brew install`; that
# is intentional for a staged template.
#
# livecheck enabled 2026-06-11: the first public release (v0.12.0)
# exists on GitHub Releases, so Homebrew can track new versions.

class Tolkin < Formula
  desc "Privacy-first AI token analyzer for agent workflows"
  homepage "https://github.com/agnelnieves/tolkin"
  version "0.11.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/agnelnieves/tolkin/releases/download/v0.11.0/tolkin-v0.11.0-darwin-arm64.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end

    on_intel do
      url "https://github.com/agnelnieves/tolkin/releases/download/v0.11.0/tolkin-v0.11.0-darwin-x64.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/agnelnieves/tolkin/releases/download/v0.11.0/tolkin-v0.11.0-linux-arm64.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end

    on_intel do
      url "https://github.com/agnelnieves/tolkin/releases/download/v0.11.0/tolkin-v0.11.0-linux-x64.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
  end

  def install
    bin.install "tolkin"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/tolkin --version")
  end

  livecheck do
    url :stable
    strategy :github_latest
  end
end
