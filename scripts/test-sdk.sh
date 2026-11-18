#!/bin/sh
# Тесты библиотеки opencode-client (aarch64) в образе sailo-rs через sb2+qemu.
#
# Зачем отдельный скрипт: крейт линкует Qt/SFOS и не собирается на хосте —
# `cargo test` нужно гонять внутри SDK-образа (как сборку RPM), где есть
# кросс-тулчейн и эмуляция aarch64. Чистый протокол (opencode-proto) для
# быстрой проверки гоняется на хосте: `make test`.
#
# Использование:
#   sg docker -c "./scripts/test-sdk.sh"          # весь lib-тест
#   sg docker -c "./scripts/test-sdk.sh -- dispatcher"  # фильтр имён тестов
set -e

# Same env overrides as mbuild.sh: PROJ, CACHE, IMAGE.
PROJ="${PROJ:-$(cd "$(dirname "$0")/.." && pwd)}"
CACHE="${CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/opencode-sailfish/cache}"
IMAGE="${IMAGE:-registry.gitlab.com/whisperfish/sailo-rs/rust-aarch64-5.1.0.11:sfos-5-1}"

mkdir -p "$CACHE/cargo"
chmod 777 "$CACHE" "$CACHE/cargo" 2>/dev/null || true

docker run --rm \
  -e HOME=/home/mersdk \
  -v "$PROJ:/src:ro" \
  -v "$CACHE:/home/mersdk/cargo-cache" \
  "$IMAGE" bash -lc '
  set -e
  mkdir -p /home/mersdk/cargo-cache
  rm -rf ~/oc-test; mkdir -p ~/oc-test
  cd /src
  tar -cf - \
    --exclude=./opencode-client/target \
    --exclude=./voice-engine/target \
    --exclude=./opencode-proto/target \
    --exclude=./.git \
    --exclude=./spike \
    --exclude=./sfos-sysroot \
    --exclude=./docker \
    --exclude=./third_party \
    . | tar -xf - -C ~/oc-test
  cd ~/oc-test
  mkdir -p third_party
  ln -sfn "$HOME/cargo-cache/whisper.cpp" third_party/whisper.cpp
  cd opencode-client
  export CARGO_TARGET_DIR=/home/mersdk/cargo-cache/cargo/debug
  # QMAKE нужен build.rs qmetaobject (версия Qt), как в rpm/*.spec %build.
  sb2 -t SailfishOS-5.1.0.11-aarch64 -- \
    env QMAKE=/usr/lib64/qt5/bin/qmake SB2_RUST_TARGET_TRIPLE=aarch64-unknown-linux-gnu \
    cargo test --lib -j 1 "$@" 2>&1 \
    | grep -vE "^warning|^[[:space:]]+\||^[[:space:]]+=|^[[:space:]]+-->|^[[:space:]]*$|note:|^Messages|^ \(WARNING\)"
' _ "$@"
