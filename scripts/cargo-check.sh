#!/bin/sh
# `cargo check` opencode-client на хосте (без SDK-контейнера).
#
# Аппаратный/тулчейновый gate — по-прежнему RPM-сборка (`./mbuild.sh`), а это
# быстрый цикл «правь → компилятор сказал → правь» для редактирования кода.
# Кросс-сборка aarch64 идёт штатно (см. opencode-client/.cargo/config.toml),
# не хватает только заголовков SFOS для C++-шима `sailors`
# (`#include <sailfishapp.h>`): берём их из локального sysroot.
#
# Использование:
#   scripts/cargo-check.sh                     # короткий формат диагностик
#   scripts/cargo-check.sh --message-format=full  # как обычно
set -e

PROJ="$(cd "$(dirname "$0")/.." && pwd)"
CLIENT="$PROJ/opencode-client"
SYSROOT="$PROJ/sfos-sysroot/targets/SailfishOS-5.1.0.11-aarch64"

if [ ! -d "$SYSROOT/usr/include/sailfishapp" ]; then
    echo "нет sysroot SFOS: $SYSROOT" >&2
    exit 1
fi

cd "$CLIENT"
if [ $# -eq 0 ]; then
    set -- --message-format short
fi
# PKG_CONFIG_* — чтобы qmetaobject брал Qt из sysroot, а не с хоста;
# CXXFLAGS — докинуть include sailfishapp (build.rs sailors хардкодит
# системный путь, который на хосте никуда не ведёт).
PKG_CONFIG_PATH= \
PKG_CONFIG_LIBDIR="$SYSROOT/usr/lib64/pkgconfig:$SYSROOT/usr/share/pkgconfig" \
PKG_CONFIG_SYSROOT_DIR="$SYSROOT" \
PKG_CONFIG_ALLOW_CROSS=1 \
CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc \
CXX_aarch64_unknown_linux_gnu=aarch64-linux-gnu-g++ \
CXXFLAGS_aarch64_unknown_linux_gnu="-I$SYSROOT/usr/include/sailfishapp" \
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
    cargo check "$@"
