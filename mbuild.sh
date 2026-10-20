#!/bin/sh
# Build harbour-opencode via mb2 in the sailo-rs Docker image (Rust + SFOS SDK).
#
# Usage:
#   sg docker -c "./mbuild.sh"            # debug — fast, no optimizations (for development)
#   sg docker -c "./mbuild.sh --release"  # release — with optimizations (for distribution)
#
# The cargo-target cache lives on the host (CACHE) and survives container runs:
# rebuilds touch only the changed crates, not everything from scratch.
set -e

PROJ=/home/jack/w/sailfish-opencode
OUT=/home/jack/.cache/opencode-sailfish/out
CACHE=/home/jack/.cache/opencode-sailfish/cache
IMAGE=registry.gitlab.com/whisperfish/sailo-rs/rust-aarch64-5.1.0.11:sfos-5-1

MODE=debug
for arg in "$@"; do
    case "$arg" in
        --release) MODE=release ;;
        -h|--help)
            echo "Сборка harbour-opencode (RPM)."
            echo "  ./mbuild.sh            — debug (по умолчанию, быстро)."
            echo "  ./mbuild.sh --release  — со всеми оптимизациями."
            exit 0 ;;
        *) echo "неизвестный флаг: $arg (допустим только --release)"; exit 1 ;;
    esac
done

mkdir -p "$OUT"
mkdir -p "$CACHE/cargo"
chmod 777 "$CACHE" "$CACHE/cargo"
chmod 777 "$OUT"
rm -f "$OUT"/*.rpm

# A fresh launcher icon for every build: random-gradient SVG with the opencode
# glyph (chevron + cursor), rasterized into icons/ (the spec packs those PNGs
# into the RPM). Static Rust binary (tools/gen-icon), no Python/cairosvg
# needed on the build host; the dispatcher picks the binary matching the
# host architecture (x86_64/i686/aarch64/armv7).
"$PROJ/tools/gen-icon/gen-icon" --out "$PROJ/icons" --svg "$PROJ/harbour-opencode.svg"

# Bump Release so the phone/repo sees each build as an update
# (standard RPM versioning: X.Y.Z-N, Release grows on every build).
SPEC="$PROJ/rpm/harbour-opencode.spec"
REL="$(sed -n 's/^Release:[[:space:]]*\([0-9][0-9]*\).*/\1/p' "$SPEC")"
sed -i "s/^Release:.*/Release:    $((REL + 1))/" "$SPEC"
echo "release: $REL -> $((REL + 1))"

docker run --rm \
  -e MBUILD_MODE="$MODE" \
  -v "$PROJ:/src:ro" \
  -v "$OUT:/out" \
  -v "$CACHE:/home/mersdk/cargo-cache" \
  "$IMAGE" bash -lc '
  set -e
  echo "MBUILD_MODE=$MBUILD_MODE"
  rm -rf ~/opencode-build
  mkdir -p ~/opencode-build
  cd /src
  tar -cf - \
    --exclude=./opencode-client/target \
    --exclude=./voice-engine/target \
    --exclude=./.git \
    --exclude=./spike \
    --exclude=./sfos-sysroot \
    --exclude=./docker \
    --exclude=./third_party \
    . | tar -xf - -C ~/opencode-build
  cd ~/opencode-build
  # Voice dependencies: whisper.cpp (STT) in $HOME/cargo-cache —
  # sb2 exposes only $HOME to the outside, so both cargo-target and whisper
  # are kept there (persistent volume), so that only changes get rebuilt.
  if [ ! -d "$HOME/cargo-cache/whisper.cpp" ]; then
    curl -fsSL -o /tmp/whisper.cpp.tar.gz \
      "https://github.com/ggml-org/whisper.cpp/archive/refs/tags/v1.5.4.tar.gz"
    tar -xzf /tmp/whisper.cpp.tar.gz -C "$HOME/cargo-cache"
    mv "$HOME/cargo-cache"/whisper.cpp-1.5.4 "$HOME/cargo-cache/whisper.cpp"
  fi
  mkdir -p third_party
  ln -sfn "$HOME/cargo-cache/whisper.cpp" third_party/whisper.cpp
  # Patch for whisper-cli: adds the CLI option --audio-ctx (FUTO approach to speed).
  if ! grep -qF -- "--audio-ctx" third_party/whisper.cpp/examples/main/main.cpp; then
    patch -p1 -d third_party/whisper.cpp < /src/packaging/whisper-cli-audio-ctx.patch
    echo "whisper-cli: патч --audio-ctx применён"
  else
    echo "whisper-cli: патч --audio-ctx уже применён"
  fi
  # Note: whisper.cpp v1.5.4 has NO OpenMP code at all (no _OPENMP, no pragma
  # omp); threading is done with pthreads and driven purely by `-t N` from
  # the app. Building with -fopenmp was verified to be a no-op (binary size
  # and NEEDED libs are unchanged, do not reintroduce it).
  ls -d third_party/whisper.cpp
  # Piper (TTS): a ready-made static aarch64 build (reference) — we unpack it
  # into the cache, then the spec installs it into %{payloaddir}/piper
  # (/home/harbour-opencode/piper, symlinked from /usr/libexec/harbour-opencode).
  if [ ! -e "$HOME/cargo-cache/piper-aarch64/piper/piper" ]; then
    curl -fsSL -o /tmp/piper-aarch64.tar.gz \
      "https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_linux_aarch64.tar.gz"
    rm -rf "$HOME/cargo-cache/piper-aarch64"
    mkdir -p "$HOME/cargo-cache/piper-aarch64"
    tar -xzf /tmp/piper-aarch64.tar.gz -C "$HOME/cargo-cache/piper-aarch64"
  fi
  find "$HOME/cargo-cache/piper-aarch64" -maxdepth 2 -name piper -type f
  ls -l "$HOME/cargo-cache/piper-aarch64/piper"
  # opencode CLI: we bundle the arm64 binary into the RPM (/home/harbour-opencode/opencode,
  # symlinked from /usr/libexec/harbour-opencode/opencode)
  # so the client does not depend on a separately installed opencode or conflict with it via PATH.
  #
  # opencode has TWO release lines (see vault/Разведка-v2.md §10):
  #   * v1.x (1.19.0..1.19.3) — "OpenCode 1": GitHub Releases + npm `latest`.
  #   * v2.0.x (2.0.28..2.0.32) — the real "OpenCode 2": CDN only, NOT in GitHub Releases.
  # 1.19.0 was a *bridge* build (v1 routes AND /api/* v2 routes, live events under
  # `session.next.*`). We target the real v2 now: 2.0.32 drops the bridge (no
  # `session.next.*`, no v1 routes) and emits the plain `session.*` v2 events the
  # client already handles. 2.0.x binaries live on the CDN, not under GitHub Releases:
  #   https://opencode.ai/files/bin/<ver>/opencode-linux-arm64.tar.gz
  OC_VERSION="2.0.32"
  # SHA-256 of the reference 2.0.32 binary (linux-arm64, extracted from the CDN tarball).
  # We check not only the version but the file itself: protection against cache substitution,
  # which could otherwise slip the wrong "opencode" into the RPM.
  OC_SHA256="4199ae2d2c96ad2a140d32cabe6a1d9280b4cd1d9d8ed533a5431f54c422693c"
  OC_CLI="$HOME/cargo-cache/opencode-cli"
  # set -e: sha256sum of a missing file exits with 1 — that is not an error
  # but a signal that the file "needs downloading" (checked in the if below).
  OC_SHA_HAVE=$(sha256sum "$OC_CLI/opencode" 2>/dev/null || true)
  OC_SHA_HAVE=${OC_SHA_HAVE%% *}
  if [ ! -e "$OC_CLI/opencode" ] \
     || [ "$(cat "$OC_CLI/opencode.version" 2>/dev/null)" != "$OC_VERSION" ] \
     || [ "$OC_SHA_HAVE" != "$OC_SHA256" ]; then
    curl -fsSL -o /tmp/opencode-linux-arm64.tar.gz \
      "https://opencode.ai/files/bin/${OC_VERSION}/opencode-linux-arm64.tar.gz"
    rm -rf "$OC_CLI"
    mkdir -p "$OC_CLI"
    tar -xzf /tmp/opencode-linux-arm64.tar.gz -C "$OC_CLI"
    printf "%s\n" "$OC_VERSION" > "$OC_CLI/opencode.version"
  fi
  echo "opencode CLI version: $(cat "$OC_CLI/opencode.version")"
  OC_SHA_NOW=$(sha256sum "$OC_CLI/opencode")
  echo "opencode CLI sha256: ${OC_SHA_NOW%% *}"
  find "$OC_CLI" -maxdepth 1 -name opencode -type f
  ls -l "$OC_CLI/opencode"
  mb2 -n -t SailfishOS-5.1.0.11-aarch64 --no-snapshot=force build
  echo "=== RPM ($MBUILD_MODE) ==="
  ls -l RPMS/*.rpm
  # Verification: the EXACT pinned opencode must end up inside the RPM.
  # In the past brp-strip trimmed the Bun binary and a "serve"-less binary ended up in the
  # RPM (1.3.14 instead of 1.18.30), even though the correct file was placed into buildroot.
  OC_RPM="$(pwd)/$(ls RPMS/*.rpm | head -1)"
  echo "проверяю opencode внутри RPM: $OC_RPM"
  rm -rf /tmp/ocverify && mkdir -p /tmp/ocverify && cd /tmp/ocverify
  rpm2cpio "$OC_RPM" | cpio -id --quiet "./home/harbour-opencode/opencode" >/dev/null 2>&1 || true
  if [ ! -f home/harbour-opencode/opencode ]; then
    echo "ОШИБКА: не удалось извлечь opencode из RPM (rpm2cpio/cpio)"
    exit 1
  fi
  OC_VERIFY="$(sha256sum home/harbour-opencode/opencode | awk "{print \$1}")"
  echo "opencode в RPM sha256: $OC_VERIFY"
  if [ "$OC_VERIFY" != "$OC_SHA256" ]; then
    echo "ОШИБКА: в RPM уехал НЕ пинированный opencode."
    echo "ожидался: $OC_SHA256"
    echo "реально:  $OC_VERIFY"
    echo "похоже, rpmbuild снова урезал бинарь — проверь %global __strip / debug_package в spec."
    exit 1
  fi
  echo "opencode в RPM: совпадает с пином — OK"
  cd - >/dev/null
  cp -v RPMS/*.rpm /out/
'

cp -v "$OUT"/*.rpm "$PROJ"