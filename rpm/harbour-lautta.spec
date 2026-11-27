# SPDX-License-Identifier: LGPL-2.1-or-later

Name:       harbour-lautta
Summary:    A file manager for Sailfish OS
Version:    1.0.0
Release:    1
License:    LGPLv2+ and ASL 2.0 and BSD and OFL
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
Requires:   qt5-qtsvg-plugin-imageformat-svg

%define debug_package %{nil}
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
install -m 0644 LICENSE %{buildroot}%{_datadir}/%{name}/LICENSE
install -D -m 0644 -t %{buildroot}%{_datadir}/%{name}/fonts fonts/*.ttf fonts/OFL.txt
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
