Name:           argon-control-center
Version:        0.2.0
Release:        1%{?dist}
Summary:        Raspberry Pi & Argon40 hardware control center
License:        MIT
URL:            https://github.com/wendelmax/argon-control-center
Source0:        %{url}/archive/v%{version}/argon-control-center-%{version}.tar.gz
Source1:        argon-control-center.sysusers

BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  gcc
BuildRequires:  pkgconfig(gl)
BuildRequires:  pkgconfig(egl)
BuildRequires:  pkgconfig(xkbcommon)
BuildRequires:  pkgconfig(wayland-client)
BuildRequires:  pkgconfig(x11)
BuildRequires:  systemd-rpm-macros

Requires:       mesa-libGL
Requires:       mesa-libEGL
Requires:       libxkbcommon
Requires:       systemd-udev
Recommends:     polkit
Recommends:     i2c-tools
Recommends:     libgpiod-utils

%global debug_package %{nil}

%description
Desktop app to monitor and configure a Raspberry Pi in an Argon40 case: live
telemetry, fan curve editor, board-aware overclocking (Pi 3/4/5), config.txt
hardware toggles, GPIO logic analyzer, serial/I2C console and argononed service
control. English and Brazilian Portuguese UI. Installs udev rules and the
i2c/gpio groups so members get hardware access without setuid binaries.

%prep
%autosetup -n argon-control-center-%{version}

%build
cargo build --release --locked

%install
sh packaging/common/install-tree.sh %{buildroot} target/release/%{name}

%pre
%sysusers_create_compat %{SOURCE1}

%post
/usr/lib/argon-control-center/setup-permissions.sh "${SUDO_USER:-}" || :

%postun
udevadm control --reload-rules >/dev/null 2>&1 || :

%files
%license LICENSE
%doc README.md
%doc %{_docdir}/%{name}/CREDITS.md
%{_bindir}/%{name}
%{_datadir}/applications/%{name}.desktop
%{_datadir}/icons/hicolor/scalable/apps/%{name}.svg
%{_metainfodir}/io.github.wendelmax.argon_control_center.metainfo.xml
%{_udevrulesdir}/60-argon-control-center.rules
%{_sysusersdir}/argon-control-center.conf
%{_modulesloaddir}/argon-control-center.conf
%dir %{_prefix}/lib/argon-control-center
%{_prefix}/lib/argon-control-center/setup-permissions.sh
%{_datadir}/plasma/plasmoids/org.argon.controlcenter.*
%{_mandir}/man1/argon-control-center.1*

%changelog
* Wed Nov 11 2026 wendelmax - 0.2.0-1
- Release 0.2.0

* Wed Nov 11 2026 wendelmax - 0.1.0-1
- Initial package
