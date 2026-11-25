# The Homebrew formula, published to JonathansDev/homebrew-tap by release.yml with the three
# @…@ values filled in. The install and upgrade tests fill them with a build of their own.
#
# keg_only keeps this copy off PATH: Homebrew's folder can be changed without a password, so
# `sudo phylode` must never find it. Only the first install runs it, by its full path; after
# that, phylode is the copy installed in /usr/local/bin, and brew upgrade hands new versions
# to the upgrade helper, which checks them before running them.
class Phylode < Formula
  desc "DNS gatekeeper that blocks ads and trackers for every device on the network"
  homepage "https://github.com/JonathansDev/Phylode"
  url "@URL@"
  version "@VERSION@"
  sha256 "@SHA256@"
  license any_of: ["MIT", "Apache-2.0"]

  keg_only "it installs itself to /usr/local/bin with sudo, and that copy is the one to run"

  depends_on arch: :arm64
  depends_on macos: :tahoe

  def install
    bin.install "phylode"
  end

  def caveats
    <<~EOS
      To finish installing, run this once and enter your password:
        sudo #{opt_bin}/phylode install

      After that, type phylode as usual. brew upgrade updates the running
      service by itself, with no password.

      Before brew uninstall phylode, remove the service first:
        sudo phylode uninstall
    EOS
  end

  test do
    assert_match "Phylode #{version}", shell_output("#{bin}/phylode --version")
  end
end
