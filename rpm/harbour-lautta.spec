# SPDX-License-Identifier: LGPL-2.1-or-later
# Built with `mb2 build` in the Sailfish OS SDK (tools/ci/build-rpm.sh).
# Dependencies are vendored before the build (make vendor); the build never
# touches the network (SPEC RS-6, PKG-1).

Name:       harbour-lautta
Summary:    A file manager for Sailfish OS
Version:    0.1.0
Release:    1
License:    LGPLv2+
URL:        https://github.com/muhnschein/lautta
Source0:    %{name}-%{version}.tar.bz2
ExclusiveArch: aarch64

BuildRequires: rust >= 1.75
BuildRequires: cargo
BuildRequires: rust-std-static-aarch64-unknown-linux-gnu
BuildRequires: pkgconfig(Qt5Core)
BuildRequires: pkgconfig(Qt5Gui)
BuildRequires: pkgconfig(Qt5Qml)
BuildRequires: pkgconfig(Qt5Quick)
BuildRequires: pkgconfig(Qt5Multimedia)
BuildRequires: pkgconfig(sailfishapp)
BuildRequires: pkgconfig(sqlite3)
BuildRequires: pkgconfig(liblzma)
BuildRequires: bzip2-devel
BuildRequires: zlib-devel
BuildRequires: qt5-qttools-linguist
BuildRequires: desktop-file-utils

Requires:   sailfishsilica-qt5 >= 0.10.9
Requires:   nemo-qml-plugin-notifications-qt5
Requires:   nemo-qml-plugin-thumbnailer-qt5
Requires:   nemo-qml-plugin-configuration-qt5
Requires:   qml(Nemo.KeepAlive)
Requires:   qt5-qtdeclarative-import-multimedia
Requires:   qt5-qtmultimedia

# No debug info in the package (HBR-6).
%define debug_package %{nil}
%define __provides_exclude_from ^%{_datadir}/.*$

%description
Lautta manages your files: documents, downloads, pictures, music, videos,
SD cards and USB drives. It works with network locations provided by
netvfs, if installed.

%prep
%setup -q -n %{name}-%{version}

%build
# Qt locations for qttypes/qmetaobject (no qmake call from build scripts
# under scratchbox2).
export QT_INCLUDE_PATH=%{_includedir}/qt5
export QT_LIBRARY_PATH=%{_libdir}
export CARGO_HOME=$PWD/.cargo-home
export CARGO_INCREMENTAL=0
export CARGO_TARGET_DIR=$PWD/target-sdk
# The system libraries Harbour allows (HBR-2).
export LIBSQLITE3_SYS_USE_PKG_CONFIG=1
export LZMA_API_STATIC=0
export RUSTFLAGS="-C link-arg=-Wl,--as-needed -C link-arg=-Wl,-z,relro,-z,now"
cargo build --release --frozen --offline --target aarch64-unknown-linux-gnu -p harbour-lautta --features sailfish
lrelease -idbased translations/*.ts

%install
rm -rf %{buildroot}
install -D -m 0755 target-sdk/aarch64-unknown-linux-gnu/release/harbour-lautta %{buildroot}%{_bindir}/%{name}
install -d %{buildroot}%{_datadir}/%{name}/qml %{buildroot}%{_datadir}/%{name}/translations
cp -r qml/* %{buildroot}%{_datadir}/%{name}/qml/
install -m 0644 translations/*.qm %{buildroot}%{_datadir}/%{name}/translations/
for size in 86x86 108x108 128x128 172x172; do
    install -D -m 0644 icons/$size/%{name}.png %{buildroot}%{_datadir}/icons/hicolor/$size/apps/%{name}.png
done
install -D -m 0644 %{name}.desktop %{buildroot}%{_datadir}/applications/%{name}.desktop
desktop-file-validate %{buildroot}%{_datadir}/applications/%{name}.desktop || :

%files
%defattr(-,root,root,-)
%{_bindir}/%{name}
%{_datadir}/%{name}
%{_datadir}/applications/%{name}.desktop
%{_datadir}/icons/hicolor/*/apps/%{name}.png
