Name:       harbour-opencode
Summary:    Native opencode client for Sailfish OS
Version:    0.1.0
Release:    8
Group:      Qt/Qt
License:    MIT
URL:        https://github.com/boshtannik/sailfish-opencode
Source0:    %{name}-%{version}.tar.bz2
Requires:   sailfishsilica-qt5 >= 0.10.9
Requires:   libsailfishapp
BuildRequires:  rust
BuildRequires:  cargo
BuildRequires:  pkgconfig(Qt5Core)
BuildRequires:  pkgconfig(Qt5Qml)
BuildRequires:  pkgconfig(Qt5Quick)
BuildRequires:  pkgconfig(sailfishapp)
BuildRequires:  qt5-qttools-linguist

# Do NOT let rpmbuild trim/strip the bundle. brp-strip (and find-debuginfo)
# trim the non-allocated sections of the Bun binary opencode, after which it
# loses the built-in CLI ("serve") and fails with "'Script not found serve'"
# (--version instead of the real version shows 1.3.14 — Bun's own version). Therefore:
%global debug_package %{nil}
%global __strip /bin/true

# Heavy *helper* payload (bundled opencode, whisper-cli, piper) is installed
# under %{payloaddir} on the /home partition — a large (tens of GB) separate
# filesystem — and only the /usr/libexec/%{name} symlink lives in /usr (on the
# small, often-full root partition). The client addresses these helpers by
# absolute /usr/libexec paths, which resolve through the symlink; the desktop
# file disables sailjail sandboxing, so /home is reachable at runtime.
#
# NOTE: the GUI launcher (harbour-opencode) MUST stay a real file in %{_bindir}.
# libsailfishapp derives the app data dir from the *executable's directory*
# (SailfishAppPriv::dataDir replaces the last path component of the exe's dir
# with "share/"). A symlinked launcher resolves to /home/%{name}, so the app
# would look for its QML under /home/share/%{name} → white screen.
%define payloaddir /home/%{name}

%description
Native client for the opencode AI coding agent. Connects to the local
opencode server over SSE and provides a Silica chat UI.

# - PREP -----------------------------------------------------------------------
%prep
%setup -q -n %{name}-%{version}

# - BUILD ----------------------------------------------------------------------
%build
cd opencode-client
MODE="${MBUILD_MODE:-debug}"
echo "harbour-opencode: MBUILD_MODE=$MODE"
export QMAKE=/usr/lib64/qt5/bin/qmake
export SB2_RUST_TARGET_TRIPLE=aarch64-unknown-linux-gnu
export CC_aarch64_unknown_linux_gnu=aarch64-meego-linux-gnu-gcc
export CXX_aarch64_unknown_linux_gnu=aarch64-meego-linux-gnu-g++
export AR_aarch64_unknown_linux_gnu=aarch64-meego-linux-gnu-ar
case "$MODE" in
  release)
    # Release: compressed code and better codegen — slow, for distribution only.
    export RUSTFLAGS="-Clink-arg=-Wl,-z,relro,-z,now -Ccodegen-units=1 -Clink-arg=-rdynamic"
    export CARGO_INCREMENTAL=0
    export CARGO_TARGET_DIR=/home/mersdk/cargo-cache/cargo/release
    cargo build --release -j 1
    ;;
  *)
    # Dev build by default: no optimizations and no codegen-units freezing.
    export RUSTFLAGS="-Clink-arg=-Wl,-z,relro,-z,now -Clink-arg=-rdynamic"
    export CARGO_INCREMENTAL=1
    export CARGO_TARGET_DIR=/home/mersdk/cargo-cache/cargo/debug
    cargo build -j 1
    ;;
esac

# whisper.cpp: static whisper-cli binary (STT). Built without OpenBLAS/COREML.
cd ../third_party/whisper.cpp
make -j1 \
  UNAME_M=generic \
  CC=aarch64-meego-linux-gnu-gcc CXX=aarch64-meego-linux-gnu-g++ \
  AR=aarch64-meego-linux-gnu-ar \
  main || true
test -x main
ls -l main

# Piper (TTS): check that the reference aarch64 build is present in the cache (downloaded by mbuild.sh).
test -x /home/mersdk/cargo-cache/piper-aarch64/piper/piper
ls -l /home/mersdk/cargo-cache/piper-aarch64/piper

# Translations: .ts → .qm for each fully translated language.
# When adding a new language (harbour-opencode-XX.ts), add it here.
cd ../../opencode-client/translations
for ts in harbour-opencode-ru.ts harbour-opencode-fi.ts harbour-opencode-uk.ts harbour-opencode-de.ts; do
  lrelease -qm "${ts%.ts}.qm" "$ts"
done
cd ../..

# - INSTALL --------------------------------------------------------------------
%install
MODE="${MBUILD_MODE:-debug}"
rm -rf %{buildroot}
# GUI launcher → real file in %{_bindir} (see the NOTE above: its location
# determines the app data dir). Heavy helpers → %{payloaddir} (on /home).
install -Dm 755 /home/mersdk/cargo-cache/cargo/$MODE/aarch64-unknown-linux-gnu/$MODE/harbour-opencode -t %{buildroot}%{_bindir}
PAYLOAD=%{buildroot}%{payloaddir}
install -Dm 755 third_party/whisper.cpp/main $PAYLOAD/whisper-cli
# Piper: the whole directory (binary + .so with RUNPATH=$ORIGIN + espeak-ng-data).
mkdir -p $PAYLOAD/piper
cp -a /home/mersdk/cargo-cache/piper-aarch64/piper/. $PAYLOAD/piper/
echo "OC-SRC md5: $(md5sum /home/mersdk/cargo-cache/opencode-cli/opencode)"
install -Dm 755 /home/mersdk/cargo-cache/opencode-cli/opencode $PAYLOAD/opencode
echo "OC-DST md5: $(md5sum $PAYLOAD/opencode)"
# /usr/libexec/%{name} is a symlink to %{payloaddir}; the client resolves its
# helpers through it.
install -d %{buildroot}%{_libexecdir}
ln -s %{payloaddir} %{buildroot}%{_libexecdir}/%{name}
install -Dm 644 opencode-client/assets/models.json -t %{buildroot}%{_datadir}/%{name}
# Launcher icons in all recommended sizes (Sailfish icon guidelines).
for size in 86x86 108x108 128x128 172x172; do
  install -Dm 644 icons/$size/harbour-opencode.png \
    %{buildroot}%{_datadir}/icons/hicolor/$size/apps/harbour-opencode.png
done
install -Dm 644 harbour-opencode.desktop -t %{buildroot}%{_datadir}/applications
install -d %{buildroot}%{_datadir}/%{name}
cp -r opencode-client/qml %{buildroot}%{_datadir}/%{name}/qml
# Translations (.qm) for the system locale (install -m sets owner/permissions).
install -d %{buildroot}%{_datadir}/%{name}/translations
install -m 644 opencode-client/translations/*.qm -t %{buildroot}%{_datadir}/%{name}/translations
# do-probe.sh must be executable (the bit is not guaranteed on copy).
chmod +x %{buildroot}%{_datadir}/%{name}/qml/do-probe.sh

desktop-file-install --delete-original    \
  --dir %{buildroot}%{_datadir}/applications    \
   %{buildroot}%{_datadir}/applications/*.desktop

# - CHECK ----------------------------------------------------------------------
%check
MODE="${MBUILD_MODE:-debug}"
if nm -D /home/mersdk/cargo-cache/cargo/$MODE/aarch64-unknown-linux-gnu/$MODE/harbour-opencode | grep " T main$" > /dev/null ; then
  echo "main symbol exists"
else
  echo "main symbol is missing"
  exit 1
fi

# - FILES ----------------------------------------------------------------------
%files
%defattr(-,root,root,-)
%{payloaddir}
%{_bindir}/harbour-opencode
%{_libexecdir}/%{name}
%{_datadir}/%{name}/qml
%{_datadir}/%{name}/translations
%{_datadir}/%{name}/models.json
%{_datadir}/applications/%{name}.desktop
%{_datadir}/icons/hicolor/86x86/apps/%{name}.png
%{_datadir}/icons/hicolor/108x108/apps/%{name}.png
%{_datadir}/icons/hicolor/128x128/apps/%{name}.png
%{_datadir}/icons/hicolor/172x172/apps/%{name}.png
