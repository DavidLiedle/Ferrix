Name:           ferrix
Version:        0.1.0
Release:        1%{?dist}
Summary:        Revolutionary Rust-based terminal multiplexer
License:        MIT OR Apache-2.0
URL:            https://github.com/davidliedle/ferrix
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  rust >= 1.70.0
BuildRequires:  cargo
BuildRequires:  openssl-devel
BuildRequires:  gcc

%description
Ferrix is a modern terminal multiplexer that combines the best features
of GNU Screen and tmux while introducing innovative capabilities only
possible with modern technology.

Features:
- Session persistence across system restarts
- Advanced window and pane management with intelligent layouts
- Vim-style copy mode with search and visual selection
- Remote session support with TLS encryption
- Plugin system for extensibility
- Git-like session versioning for checkpoint and rollback
- GPU-accelerated rendering (when available)

%prep
%setup -q

%build
cargo build --release

%install
install -D -m 755 target/release/ferrix %{buildroot}%{_bindir}/ferrix
install -D -m 644 docs/ferrix.1 %{buildroot}%{_mandir}/man1/ferrix.1
install -D -m 644 completions/ferrix.bash %{buildroot}%{_datadir}/bash-completion/completions/ferrix
install -D -m 644 completions/_ferrix %{buildroot}%{_datadir}/zsh/site-functions/_ferrix
install -D -m 644 completions/ferrix.fish %{buildroot}%{_datadir}/fish/vendor_completions.d/ferrix.fish

%files
%license LICENSE
%doc README.md
%{_bindir}/ferrix
%{_mandir}/man1/ferrix.1*
%{_datadir}/bash-completion/completions/ferrix
%{_datadir}/zsh/site-functions/_ferrix
%{_datadir}/fish/vendor_completions.d/ferrix.fish

%changelog
* Sat Jan 27 2025 David Liedle <david@example.com> - 0.1.0-1
- Initial release of Ferrix