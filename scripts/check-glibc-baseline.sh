#!/usr/bin/env bash
set -euo pipefail

baseline="${GLIBC_BASELINE:-2.31}"

if [ "$#" -eq 0 ]; then
  echo "usage: GLIBC_BASELINE=2.31 $0 <binary>..." >&2
  exit 2
fi

if ! command -v readelf >/dev/null 2>&1; then
  echo "readelf is required to verify the glibc baseline" >&2
  exit 2
fi

if ! [[ "$baseline" =~ ^[0-9]+\.[0-9]+$ ]]; then
  echo "invalid GLIBC_BASELINE: $baseline" >&2
  exit 2
fi

for binary in "$@"; do
  if [ ! -f "$binary" ]; then
    echo "binary does not exist: $binary" >&2
    exit 2
  fi

  versions="$({ readelf --version-info "$binary" || true; } \
    | grep -oE 'GLIBC_[0-9]+(\.[0-9]+)+' \
    | sed 's/^GLIBC_//' \
    | sort -Vu)"

  if [ -z "$versions" ]; then
    echo "no glibc symbol versions found in GNU/Linux binary: $binary" >&2
    exit 1
  fi

  newest="$(printf '%s\n' "$versions" | tail -n 1)"
  highest="$(printf '%s\n%s\n' "$baseline" "$newest" | sort -Vu | tail -n 1)"
  if [ "$highest" != "$baseline" ]; then
    echo "$binary requires GLIBC_$newest, newer than supported baseline GLIBC_$baseline" >&2
    exit 1
  fi

  echo "$binary: maximum required glibc version GLIBC_$newest (baseline GLIBC_$baseline)"
done
