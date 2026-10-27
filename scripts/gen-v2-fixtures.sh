#!/bin/sh
# Обновление вендоренных фикстур v2 из живой установки opencode 2.x.
#
# Зачем: тесты покрытия API v2 (события + формы) сверяются с настоящим сервером,
# но должны гоняться оффлайн и детерминированно. Поэтому две фикстуры
# коммитятся в opencode-client/tests/fixtures/:
#   - v2-events-2.1.4.txt   — реестр имён событий (то, что сервер шлёт в SSE)
#   - openapi-2.1.4.json    — OpenAPI 3.1 спека (формы запросов/ответов /api/*)
# Этот скрипт пересобирает их при смене версии opencode. Тесты НЕ ходят в сеть.
#
# Источники (оба требуют распакованный arm64-бинарь и/или запущенный сервер):
#   1. События: `strings opencode | grep -oE 'type:"[a-z][a-z0-9._-]*"'` —
#      в 2.1.4 каждый эмиттер объявлен как `Fi({type:"session.step.started",…})`.
#      Это точный реестр; спеку OpenAPI события НЕ перечисляет (там
#      V2EventEncoded = просто `string`).
#   2. OpenAPI: `GET /openapi.json` с работающего `opencode serve` (под Basic-auth).
#
# Использование:
#   scripts/gen-v2-fixtures.sh <opencode-binary> [base-url] [user:pass]
# Пример:
#   scripts/gen-v2-fixtures.sh /tmp/opencode/v214/opencode http://127.0.0.1:43999 opencode:pw
#
# Без base-url обновляется только список событий; без бинаря — только спека.
set -e

BIN="$1"
BASE="${2:-}"
AUTH="${3:-}"
OUT="$(dirname "$0")/../opencode-client/tests/fixtures"
VERSION="${OC_FIXTURE_VERSION:-2.1.4}"

mkdir -p "$OUT"

if [ -n "$BIN" ]; then
    [ -f "$BIN" ] || { echo "нет бинаря: $BIN" >&2; exit 1; }
    echo "==> события из $BIN"
    strings -n 5 "$BIN" \
        | grep -oE 'type:"[a-z][a-z0-9._-]*"' \
        | sed -E 's/type:"//; s/"$//' \
        | grep -E '\.' \
        | sort -u > "$OUT/v2-events-$VERSION.txt"
    echo "    $(wc -l < "$OUT/v2-events-$VERSION.txt") имён -> v2-events-$VERSION.txt"
fi

if [ -n "$BASE" ]; then
    echo "==> OpenAPI из $BASE"
    if [ -n "$AUTH" ]; then
        curl -sf -u "$AUTH" "$BASE/openapi.json" -o "$OUT/openapi-$VERSION.json"
    else
        curl -sf "$BASE/openapi.json" -o "$OUT/openapi-$VERSION.json"
    fi
    echo "    $(wc -c < "$OUT/openapi-$VERSION.json") байт -> openapi-$VERSION.json"
fi
