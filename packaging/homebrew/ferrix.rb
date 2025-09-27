# Homebrew Formula for Ferrix

class Ferrix < Formula
  desc "Revolutionary Rust-based terminal multiplexer"
  homepage "https://github.com/davidliedle/ferrix"
  version "0.1.0"

  if OS.mac?
    if Hardware::CPU.arm?
      url "https://github.com/davidliedle/ferrix/releases/download/v#{version}/ferrix-#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER_SHA256_AARCH64"
    else
      url "https://github.com/davidliedle/ferrix/releases/download/v#{version}/ferrix-#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER_SHA256_X86_64"
    end
  elsif OS.linux?
    if Hardware::CPU.arm?
      url "https://github.com/davidliedle/ferrix/releases/download/v#{version}/ferrix-#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "PLACEHOLDER_SHA256_LINUX_AARCH64"
    else
      url "https://github.com/davidliedle/ferrix/releases/download/v#{version}/ferrix-#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "PLACEHOLDER_SHA256_LINUX_X86_64"
    end
  end

  def install
    bin.install "ferrix"

    # Install man pages
    man1.install "docs/ferrix.1" if File.exist?("docs/ferrix.1")

    # Install shell completions
    bash_completion.install "completions/ferrix.bash" if File.exist?("completions/ferrix.bash")
    zsh_completion.install "completions/_ferrix" if File.exist?("completions/_ferrix")
    fish_completion.install "completions/ferrix.fish" if File.exist?("completions/ferrix.fish")
  end

  def caveats
    <<~EOS
      Ferrix has been installed!

      To get started:
        ferrix new              # Create a new session
        ferrix attach           # Attach to existing session
        ferrix list             # List all sessions
        ferrix help             # Show help

      Configuration file: ~/.ferrix/config.toml
      RC file: ~/.ferrixrc
    EOS
  end

  test do
    system "#{bin}/ferrix", "--version"
  end
end