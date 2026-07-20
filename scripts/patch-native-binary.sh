#!/usr/bin/env bash
# Point the release binary at the system dynamic linker and apt library paths.
#
# Nixpacks builds with a nix-provided gcc/ld, so the resulting ELF often has a
# /nix/store/... interpreter. That linker does not search Ubuntu's
# /usr/lib/x86_64-linux-gnu, which is why runtime fails with:
#   libglib-2.0.so.0: cannot open shared object file
# even when apt has installed the package into the image.
#
# Do NOT put nix libs on LD_LIBRARY_PATH (glibc clash / __vdso_gettimeofday).

set -euo pipefail

resolve_bin() {
  if [[ -n "${1:-}" && -x "$1" ]]; then
    printf '%s\n' "$1"
    return
  fi
  if [[ -x bin/pixtimize ]]; then
    printf '%s\n' bin/pixtimize
    return
  fi
  if [[ -x target/release/pixtimize ]]; then
    printf '%s\n' target/release/pixtimize
    return
  fi
  echo "error: pixtimize binary not found (looked in bin/ and target/release/)" >&2
  exit 1
}

resolve_interpreter() {
  local candidate
  for candidate in \
    /lib64/ld-linux-x86-64.so.2 \
    /lib/x86_64-linux-gnu/ld-linux-x86-64.so.2 \
    /lib/ld-linux-x86-64.so.2
  do
    if [[ -f "$candidate" ]]; then
      printf '%s\n' "$candidate"
      return
    fi
  done
  echo "error: system dynamic linker (ld-linux) not found" >&2
  exit 1
}

BIN="$(resolve_bin "${1:-}")"
INTERP="$(resolve_interpreter)"
RPATH="/usr/lib/x86_64-linux-gnu:/lib/x86_64-linux-gnu:/usr/lib"

patchelf --set-interpreter "$INTERP" --set-rpath "$RPATH" "$BIN"

# Keep ./bin/pixtimize in sync when we patched the cargo output path.
if [[ "$BIN" == target/release/pixtimize ]]; then
  mkdir -p bin
  cp -f "$BIN" bin/pixtimize
fi

echo "patched $BIN"
echo "  interpreter: $INTERP"
echo "  rpath:       $RPATH"
if command -v readelf >/dev/null 2>&1; then
  readelf -l "$BIN" | awk '/Requesting program interpreter/ {print "  " $0}'
fi
