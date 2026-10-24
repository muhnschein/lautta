# SPDX-License-Identifier: LGPL-2.1-or-later
# Built with `mb2 build` in the Sailfish OS SDK (tools/ci/build-rpm.sh).
# The binary needs rustc 1.95, which no SDK target ships yet: it is
# cross-built before mb2 with the host's cargo against the SDK's gcc, Qt and
# sysroot (tools/ci/cross-build.sh) and installed from prebuilt/. The build
# never touches the network (SPEC RS-6, PKG-1).

Name:       harbour-lautta
Summary:    A file manager for Sailfish OS
Version:    0.1.0
Release:    1
License:    LGPLv2+
URL:        https://github.com/muhnschein/lautta
Source0:    %{name}-%{version}.tar.bz2
ExclusiveArch: aarch64

BuildRequires: pkgconfig(Qt5Core)
BuildRequires: pkgconfig(Qt5Qml)
BuildRequires: pkgconfig(Qt5Quick)
BuildRequires: pkgconfig(Qt5Multimedia)
BuildRequires: pkgconfig(sailfishapp)
BuildRequires: qt5-qttools-linguist
BuildRequires: desktop-file-utils

Requires:   sailfishsilica-qt5 >= 0.10.9
Requires:   nemo-qml-plugin-notifications-qt5
Requires:   nemo-qml-plugin-thumbnailer-qt5
Requires:   nemo-qml-plugin-configuration-qt5
Requires:   qml(Nemo.KeepAlive)
Requires:   qt5-qtdeclarative-import-multimedia
Requires:   qt5-qtmultimedia
# The app's own icons are SVG files.
Requires:   qt5-qtsvg-plugin-imageformat-svg

# No debug info in the package (HBR-6).
%define debug_package %{nil}
# Reproducible package (PKG-4, doc/VERIFYING.md). SOURCE_DATE_EPOCH set
# (tools/ci/build-rpm.sh: commit's time): build time = it, file dates clamped
# to it. Host name fixed either way: rpm records it.
%define use_source_date_epoch_as_buildtime 1
%define clamp_mtime_to_source_date_epoch 1
%define _buildhost lautta-build
%define __provides_exclude_from ^%{_datadir}/.*$

%description
Lautta manages your files: documents, downloads, pictures, music, videos,
SD cards and USB drives. It works with network locations provided by
netvfs, if installed.

%prep
%setup -q -n %{name}-%{version}

%build
test -x prebuilt/harbour-lautta || { echo "prebuilt/harbour-lautta missing: run tools/ci/cross-build.sh" >&2; exit 1; }
lrelease -idbased translations/*.ts

%install
rm -rf %{buildroot}
install -D -m 0755 prebuilt/harbour-lautta %{buildroot}%{_bindir}/%{name}
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
