#!/bin/bash
# Build and package Ferrix for multiple platforms

set -e

VERSION=$(grep '^version' Cargo.toml | head -1 | cut -d'"' -f2)
DIST_DIR="dist"

echo "Building Ferrix v${VERSION} for multiple platforms..."

# Clean previous builds
rm -rf "${DIST_DIR}"
mkdir -p "${DIST_DIR}"

# Function to build for a specific target
build_target() {
    local target=$1
    local output_name=$2

    echo "Building for ${target}..."

    if rustup target list | grep -q "${target} (installed)"; then
        cargo build --release --target "${target}"

        # Create package directory
        local pkg_dir="${DIST_DIR}/${output_name}"
        mkdir -p "${pkg_dir}"

        # Copy binary
        if [[ "${target}" == *"windows"* ]]; then
            cp "target/${target}/release/ferrix.exe" "${pkg_dir}/"
        else
            cp "target/${target}/release/ferrix" "${pkg_dir}/"
            chmod +x "${pkg_dir}/ferrix"
        fi

        # Copy documentation and completions
        cp README.md "${pkg_dir}/"
        cp LICENSE "${pkg_dir}/"
        cp -r docs "${pkg_dir}/"
        cp -r completions "${pkg_dir}/"

        # Create config directory with example config
        mkdir -p "${pkg_dir}/config"
        cat > "${pkg_dir}/config/example.toml" << EOF
# Example Ferrix configuration
prefix = "C-b"
mouse = true
base_index = 1
history_limit = 10000

[colors]
status_bg = "black"
status_fg = "white"
active_border = "green"
EOF

        # Create tarball or zip
        cd "${DIST_DIR}"
        if [[ "${target}" == *"windows"* ]]; then
            zip -r "${output_name}.zip" "${output_name}"
        else
            tar czf "${output_name}.tar.gz" "${output_name}"
        fi
        cd ..

        echo "Package created: ${DIST_DIR}/${output_name}.tar.gz"
    else
        echo "Target ${target} not installed. Install with: rustup target add ${target}"
    fi
}

# Build for multiple platforms
build_target "x86_64-unknown-linux-gnu" "ferrix-${VERSION}-x86_64-linux"
build_target "x86_64-apple-darwin" "ferrix-${VERSION}-x86_64-macos"
build_target "aarch64-apple-darwin" "ferrix-${VERSION}-aarch64-macos"
build_target "x86_64-pc-windows-msvc" "ferrix-${VERSION}-x86_64-windows"
build_target "aarch64-unknown-linux-gnu" "ferrix-${VERSION}-aarch64-linux"

# Generate checksums
cd "${DIST_DIR}"
sha256sum *.tar.gz *.zip > SHA256SUMS 2>/dev/null || true
cd ..

echo "Build complete! Packages are in ${DIST_DIR}/"
echo ""
echo "Generated packages:"
ls -lh "${DIST_DIR}/"

# Create Homebrew tap update
if command -v brew &> /dev/null; then
    echo ""
    echo "Updating Homebrew formula..."

    # Get SHA256 for macOS builds
    if [ -f "${DIST_DIR}/ferrix-${VERSION}-x86_64-macos.tar.gz" ]; then
        SHA_X86=$(sha256sum "${DIST_DIR}/ferrix-${VERSION}-x86_64-macos.tar.gz" | cut -d' ' -f1)
        sed -i.bak "s/PLACEHOLDER_SHA256_X86_64/${SHA_X86}/g" packaging/homebrew/ferrix.rb
    fi

    if [ -f "${DIST_DIR}/ferrix-${VERSION}-aarch64-macos.tar.gz" ]; then
        SHA_ARM=$(sha256sum "${DIST_DIR}/ferrix-${VERSION}-aarch64-macos.tar.gz" | cut -d' ' -f1)
        sed -i.bak "s/PLACEHOLDER_SHA256_AARCH64/${SHA_ARM}/g" packaging/homebrew/ferrix.rb
    fi

    rm packaging/homebrew/ferrix.rb.bak 2>/dev/null || true
fi

echo ""
echo "Release build complete!"
echo ""
echo "Next steps:"
echo "1. Test the packages locally"
echo "2. Create a GitHub release and upload the packages"
echo "3. Update package managers (Homebrew, AUR, etc.)"
echo "4. Announce the release"