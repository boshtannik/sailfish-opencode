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
#
# Про версию rustc: дистрибутивный тулчейн Debian (1.85) слишком старый для
# текущего дерева зависимостей (icu_* 2.3 требует >= 1.88). В SDK-образе
# сборки стоит 1.89, поэтому берём rustup-тулчейн (shims в ~/.cargo/bin) —
# он новее и ближе к настоящей сборке. Без rustup скрипт честно скажет,
# что версия мала.
set -e

# rustup-шимы впереди системных: `cargo`/`rustc` должны быть из rustup,
# а не из /usr/bin (apt-версия).
if [ -x "$HOME/.cargo/bin/cargo" ]; then
    PATH="$HOME/.cargo/bin:$PATH"
    export PATH
fi

MIN_RUSTC="1.88"
rustc_ver="$(rustc -V | awk '{print $2}')"
# sort -V: если минимальная версия оказалась не первой — текущая старше.
if [ "$(printf '%s\n%s\n' "$MIN_RUSTC" "$rustc_ver" | sort -V | head -n 1)" != "$MIN_RUSTC" ]; then
    echo "rustc $rustc_ver слишком старый: нужно >= $MIN_RUSTC (icu_* 2.3)." >&2
    echo "Поставь rustup-тулчейн (stable уже подходит): ~/.cargo/bin в PATH." >&2
    exit 1
fi

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
