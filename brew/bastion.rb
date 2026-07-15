# SPDX-License-Identifier: Apache-2.0 OR MIT
#
# Homebrew formula for TPT Bastion. Install with:
#   brew install tpt/tap/bastion
# or tap + install via the repo's custom tap once published.

class Bastion < Formula
  desc "Next-generation Infrastructure-as-Code platform"
  homepage "https://bastion.tpt.dev"
  url "https://github.com/tpt/bastion/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "REPLACE_WITH_SOURCE_TARBALL_SHA256"
  license "Apache-2.0 OR MIT"
  version "0.1.0"

  depends_on "rust" => :build

  def install
    system "cargo", "build", "--release", "--bin", "bastion"
    bin.install "target/release/bastion"
  end

  test do
    assert_match "bastion", shell_output("#{bin}/bastion --help")
  end
end
