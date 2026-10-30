# Spec for building Griffel RPM packages. The version fields below track
# the git tag (v0.1.0-beta.1 -> Version 0.1.0~beta1); bump them together
# with Cargo.toml, debian/changelog and the AUR PKGBUILD on each release.
Name:           griffel
Version:        0.1.0~beta1
Release:        1%{?dist}
Summary:        Distraction-free Markdown editor for the Linux desktop

License:        MIT
URL:            https://github.com/hakergeniusz/griffel
Source0:        %{url}/archive/refs/tags/v0.1.0-beta.1/griffel-0.1.0-beta.1.tar.gz

BuildRequires:  cargo
BuildRequires:  pkgconfig(gtk4) >= 4.10
BuildRequires:  pkgconfig(libadwaita-1) >= 1.4
BuildRequires:  pkgconfig(gtksourceview-5) >= 5.4
BuildRequires:  pkgconfig(webkitgtk-6.0)
BuildRequires:  pkgconfig(enchant-2)
# No -debuginfo/-debugsource subpackages: the release pipeline ships the
# main RPM only.
%define debug_package %{nil}
Recommends:     pandoc

%global _description %{expand:
Griffel shows your document as a centered column of plain Markdown while
the interface fades away as you write, and returns when you need it. It
offers live HTML preview, in-place markup highlighting, Focus and Hemingway
writing modes, spell checking, peek popovers, statistics, crash-recovery
snapshots, and export to PDF, HTML, ODT and more through Pandoc.}

%description %{_description}

%prep
%autosetup -n griffel-0.1.0-beta.1

%build
cargo build --release --locked

%install
install -Dm755 target/release/griffel %{buildroot}%{_bindir}/griffel
install -Dm644 data/io.github.hakergeniusz.Griffel.desktop \
  %{buildroot}%{_datadir}/applications/io.github.hakergeniusz.Griffel.desktop
install -Dm644 data/io.github.hakergeniusz.Griffel.service \
  %{buildroot}%{_datadir}/dbus-1/services/io.github.hakergeniusz.Griffel.service
install -Dm644 data/io.github.hakergeniusz.Griffel.metainfo.xml \
  %{buildroot}%{_datadir}/metainfo/io.github.hakergeniusz.Griffel.metainfo.xml
install -Dm644 resources/icons/io.github.hakergeniusz.Griffel.svg \
  %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/io.github.hakergeniusz.Griffel.svg
install -Dm644 resources/icons/io.github.hakergeniusz.Griffel-symbolic.svg \
  %{buildroot}%{_datadir}/icons/hicolor/symbolic/apps/io.github.hakergeniusz.Griffel-symbolic.svg

%check
./target/release/griffel --version

%files
%license LICENSE
%doc README.md
%{_bindir}/griffel
%{_datadir}/applications/io.github.hakergeniusz.Griffel.desktop
%{_datadir}/dbus-1/services/io.github.hakergeniusz.Griffel.service
%{_datadir}/metainfo/io.github.hakergeniusz.Griffel.metainfo.xml
%{_datadir}/icons/hicolor/scalable/apps/io.github.hakergeniusz.Griffel.svg
%{_datadir}/icons/hicolor/symbolic/apps/io.github.hakergeniusz.Griffel-symbolic.svg

%changelog
* Fri Oct 30 2026 hakergeniusz <hakergeniusz@users.noreply.github.com> - 0.1.0~beta1-1
- First public beta
