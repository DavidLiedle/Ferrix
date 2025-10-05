Name:           ferrix
Version:        0.11.0
Release:        1%{?dist}
Summary:        Modern terminal multiplexer with GPU acceleration

License:        MIT
URL:            https://github.com/davidliedle/Ferrix
Source0:        https://github.com/davidliedle/Ferrix/archive/v%{version}/%{name}-%{version}.tar.gz

BuildRequires:  rust >= 1.70
BuildRequires:  cargo
BuildRequires:  gcc
BuildRequires:  cmake
BuildRequires:  openssl-devel
BuildRequires:  pkg-config

Requires:       glibc
Recommends:     git
Suggests:       mesa-libGL

%description
Ferrix is a modern terminal multiplexer that combines the best features
of GNU Screen and tmux, while adding innovative capabilities only possible
with modern technology.

Features include:
- GPU-accelerated rendering for smooth performance
- Session versioning with Git-like branching and merging
- Per-session configurations
- Plugin marketplace with WASM support
- Vim and Emacs input modes
- Advanced status bar with system monitoring
- Layout presets for common workflows
- Copy mode with clipboard integration

%prep
%autosetup -n Ferrix-%{version}

%build
cargo build --release --locked

# Build with GPU feature if Mesa is available
%if 0%{?fedora} || 0%{?rhel} >= 8
cargo build --release --locked --features gpu
%endif

%install
# Install binary
install -D -m 755 target/release/ferrix %{buildroot}%{_bindir}/ferrix

# Install documentation
install -D -m 644 README.md %{buildroot}%{_docdir}/%{name}/README.md
install -D -m 644 FEATURES.md %{buildroot}%{_docdir}/%{name}/FEATURES.md
install -D -m 644 CHANGELOG.md %{buildroot}%{_docdir}/%{name}/CHANGELOG.md

# Install license
install -D -m 644 LICENSE %{buildroot}%{_licensedir}/%{name}/LICENSE

# Install shell completions
install -D -m 644 completions/ferrix.bash %{buildroot}%{_datadir}/bash-completion/completions/ferrix
install -D -m 644 completions/ferrix.fish %{buildroot}%{_datadir}/fish/vendor_completions.d/ferrix.fish
install -D -m 644 completions/_ferrix %{buildroot}%{_datadir}/zsh/site-functions/_ferrix

# Install systemd service
install -D -m 644 systemd/ferrix.service %{buildroot}%{_userunitdir}/ferrix.service

# Install default configuration
install -D -m 644 config/default.toml %{buildroot}%{_datadir}/%{name}/config/default.toml

%check
cargo test --release --locked

%files
%license LICENSE
%doc README.md FEATURES.md CHANGELOG.md
%{_bindir}/ferrix
%{_datadir}/bash-completion/completions/ferrix
%{_datadir}/fish/vendor_completions.d/ferrix.fish
%{_datadir}/zsh/site-functions/_ferrix
%{_userunitdir}/ferrix.service
%{_datadir}/%{name}/config/default.toml

%changelog
* Thu Jan 04 2024 David Liedle <david@liedle.com> - 0.11.0-1
- GPU acceleration support
- Session versioning with Git-like branching
- Plugin marketplace infrastructure
- Vim and Emacs input modes
- Enhanced status bar with system monitoring

* Wed Dec 20 2023 David Liedle <david@liedle.com> - 0.10.2-1
- Critical daemonization fix for macOS
- Fixed directory creation issues
- Improved error messages

* Mon Dec 18 2023 David Liedle <david@liedle.com> - 0.10.0-1
- Initial RPM release