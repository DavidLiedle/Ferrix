class Ferrix < Formula
  desc "Modern terminal multiplexer combining Screen and tmux features"
  homepage "https://github.com/davidliedle/Ferrix"
  url "https://github.com/davidliedle/Ferrix/archive/refs/tags/v0.11.0.tar.gz"
  sha256 "PLACEHOLDER_SHA256"  # To be updated with actual checksum
  license "MIT"
  head "https://github.com/davidliedle/Ferrix.git", branch: "main"

  depends_on "rust" => :build
  depends_on "cmake" => :build
  depends_on "pkg-config" => :build

  def install
    system "cargo", "install", *std_cargo_args

    # Install shell completions
    bash_completion.install "completions/ferrix.bash"
    fish_completion.install "completions/ferrix.fish"
    zsh_completion.install "completions/_ferrix"

    # Install man pages
    man1.install "doc/ferrix.1"

    # Install documentation
    doc.install "README.md", "FEATURES.md", "CHANGELOG.md"
  end

  service do
    run [opt_bin/"ferrix", "server"]
    keep_alive true
    log_path var/"log/ferrix.log"
    error_log_path var/"log/ferrix.error.log"
  end

  test do
    # Test that ferrix binary exists and runs
    system "#{bin}/ferrix", "--version"

    # Test server can start
    pid = fork do
      exec "#{bin}/ferrix", "server"
    end
    sleep 2
    Process.kill("TERM", pid)
    Process.wait(pid)

    # Test creating a session works
    system "#{bin}/ferrix", "new", "-s", "test-session", "--detached"
    system "#{bin}/ferrix", "kill", "test-session"
  end
end