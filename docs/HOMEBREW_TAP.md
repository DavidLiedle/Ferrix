# Homebrew Tap Setup Guide

This guide explains how to set up and publish the Ferrix Homebrew tap.

## What is a Homebrew Tap?

A Homebrew tap is a third-party repository that allows users to install your software via `brew install`. It's essentially a GitHub repository with formula files.

## Setup Steps

### 1. Create the Tap Repository

Create a new GitHub repository named `homebrew-ferrix`:

```bash
# Repository name MUST follow the pattern: homebrew-<name>
# In our case: homebrew-ferrix
```

**Repository URL**: `https://github.com/davidliedle/homebrew-ferrix`

### 2. Prepare the Formula

The formula is already created at `Formula/ferrix.rb`. After creating a v1.0.0 release:

1. Create a GitHub release with tag `v1.0.0`
2. GitHub will automatically create a tarball at:
   `https://github.com/davidliedle/Ferrix/archive/refs/tags/v1.0.0.tar.gz`
3. Calculate the SHA256 checksum:

```bash
# Download and calculate SHA256
curl -L https://github.com/davidliedle/Ferrix/archive/refs/tags/v1.0.0.tar.gz | shasum -a 256
```

4. Update the formula with the actual SHA256:

```ruby
sha256 "ACTUAL_SHA256_HERE"
```

### 3. Push Formula to Tap Repository

```bash
# Clone the homebrew-ferrix repo
git clone https://github.com/davidliedle/homebrew-ferrix
cd homebrew-ferrix

# Copy the formula
cp path/to/Ferrix/Formula/ferrix.rb Formula/ferrix.rb

# Commit and push
git add Formula/ferrix.rb
git commit -m "Add ferrix formula v1.0.0"
git push origin main
```

### 4. Test the Formula

```bash
# Test installing from the tap
brew tap davidliedle/ferrix
brew install ferrix

# Verify installation
ferrix --version

# Test uninstall
brew uninstall ferrix
brew untap davidliedle/ferrix
```

### 5. Submit to Homebrew Core (Optional)

Once Ferrix is stable and widely used, you can submit it to Homebrew's main repository:

```bash
# Requirements for Homebrew Core:
# - 75+ stars on GitHub
# - 30+ forks
# - Proven track record (6+ months)
# - No major issues
# - Active maintenance

# Submit via pull request to homebrew/homebrew-core
```

## User Installation

Once the tap is published, users can install with:

```bash
# One-time tap setup
brew tap davidliedle/ferrix

# Install
brew install ferrix

# Or install directly (without explicit tap)
brew install davidliedle/ferrix/ferrix
```

## Updating the Formula

When releasing a new version:

1. Create a new GitHub release (e.g., v1.1.0)
2. Calculate new SHA256 checksum
3. Update the formula in homebrew-ferrix repository:

```bash
cd homebrew-ferrix
# Edit Formula/ferrix.rb
# Update version and SHA256
git add Formula/ferrix.rb
git commit -m "Update ferrix to v1.1.0"
git push origin main
```

4. Users update with:

```bash
brew update
brew upgrade ferrix
```

## Formula Components Explained

```ruby
class Ferrix < Formula
  desc "..."                    # Short description (one line)
  homepage "..."                # Project homepage
  url "..."                     # Download URL (tarball)
  sha256 "..."                  # SHA256 checksum for security
  license "..."                 # Software license
  head "...", branch: "main"    # Install from HEAD (development)

  depends_on "rust" => :build   # Build-time dependency

  def install
    # Build and install commands
  end

  def caveats
    # Post-install message to users
  end

  test do
    # Automated tests for CI
  end
end
```

## Troubleshooting

### Formula Audit

Test your formula before publishing:

```bash
brew audit --strict --online Formula/ferrix.rb
brew style Formula/ferrix.rb
```

### Installation Issues

If users report installation problems:

```bash
# Check formula syntax
brew audit ferrix

# Verbose install for debugging
brew install --verbose --debug ferrix

# Clean install
brew uninstall ferrix
brew cleanup ferrix
rm -rf $(brew --cache)/ferrix*
brew install ferrix
```

## Automation with GitHub Actions (Future)

Create `.github/workflows/homebrew.yml`:

```yaml
name: Update Homebrew Formula

on:
  release:
    types: [published]

jobs:
  update-formula:
    runs-on: ubuntu-latest
    steps:
      - name: Update Homebrew Formula
        uses: dawidd6/action-homebrew-bump-formula@v3
        with:
          token: ${{ secrets.HOMEBREW_TAP_TOKEN }}
          formula: ferrix
          tap: davidliedle/homebrew-ferrix
```

## Resources

- [Homebrew Formula Cookbook](https://docs.brew.sh/Formula-Cookbook)
- [Homebrew Acceptable Formulae](https://docs.brew.sh/Acceptable-Formulae)
- [How to Create Homebrew Tap](https://docs.brew.sh/How-to-Create-and-Maintain-a-Tap)

## Checklist

- [ ] Create GitHub release v1.0.0
- [ ] Calculate SHA256 checksum
- [ ] Update formula with correct SHA256
- [ ] Create homebrew-ferrix repository
- [ ] Push formula to tap
- [ ] Test installation locally
- [ ] Test on clean macOS system
- [ ] Test on Linux
- [ ] Update README with tap instructions
- [ ] Announce tap availability
