# Homebrew Formula for Ferrix
# To install: brew install davidliedle/ferrix/ferrix
# After publishing: brew install ferrix

class Ferrix < Formula
  desc "Modern terminal multiplexer with enterprise-grade reliability"
  homepage "https://github.com/davidliedle/Ferrix"
  url "https://github.com/davidliedle/Ferrix/archive/refs/tags/v0.21.1.tar.gz"
  sha256 "PLACEHOLDER_SHA256" # Will be updated upon release
  license "MIT OR Apache-2.0"
  head "https://github.com/davidliedle/Ferrix.git", branch: "main"

  depends_on "rust" => :build

  def install
    # Build with default features (minimal build)
    system "cargo", "install", *std_cargo_args

    # Generate shell completions
    generate_completions_from_executable(bin/"ferrix", "completions")

    # Install documentation
    doc.install "README.md"
    doc.install "docs"
  end

  def caveats
    <<~EOS
      Ferrix has been installed!

      Quick Start:
        1. Start a new session:    ferrix new -s my-session
        2. List sessions:          ferrix list
        3. Attach to session:      ferrix attach -t my-session
        4. Detach from session:    Press Ctrl-b then d

      Configuration:
        Default config location: ~/.config/ferrix/config.toml
        Generate default config: ferrix config init

      For more help:
        - Run: ferrix --help
        - Documentation: #{doc}
        - Online: https://github.com/davidliedle/Ferrix

      Shell Completions have been installed:
        - Bash:       #{bash_completion}/ferrix
        - Zsh:        #{zsh_completion}/_ferrix
        - Fish:       #{fish_completion}/ferrix.fish
    EOS
  end

  test do
    # Test that the binary runs
    assert_match "ferrix 0.21.1", shell_output("#{bin}/ferrix --version")

    # Test that help works
    assert_match "Modern terminal multiplexer", shell_output("#{bin}/ferrix --help")

    # Test server mode (should exit cleanly)
    system "#{bin}/ferrix", "server", "--help"
  end
end
