#!/usr/bin/env bash
set -euo pipefail

rustc_bin=$1
shift

# Force cargo invocations that request the i686-win7-windows-msvc target to
# build using the MinGW-compatible toolchain instead, which can be cross-linked
# on this Linux host.
next_is_target=0
args=()
for raw in "$@"; do
  if [[ ${next_is_target} -eq 1 ]]; then
    if [[ "$raw" == "i686-win7-windows-msvc" ]]; then
      args+=("i686-win7-windows-gnu")
    else
      args+=("${raw}")
    fi
    next_is_target=0
    continue
  fi

  case "$raw" in
    --target)
      next_is_target=1
      args+=("--target")
      ;;
    --target=i686-win7-windows-msvc)
      args+=("--target=i686-win7-windows-gnu")
      ;;
    *)
      args+=("${raw}")
      ;;
  esac
done

exec "${rustc_bin}" "${args[@]}"
