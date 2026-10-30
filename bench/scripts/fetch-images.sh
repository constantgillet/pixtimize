#!/usr/bin/env bash
# Downloads a small, fixed set of sample photos (Unsplash licence via picsum.photos).
set -euo pipefail
cd "$(dirname "$0")/../images"

fetch() { [[ -s $2 ]] || curl -fsSL -o "$2" "$1"; echo "$2 $(wc -c < "$2" | tr -d ' ') bytes"; }

fetch https://picsum.photos/id/1015/4000/2667.jpg large-4000.jpg   # ~10 MP JPEG
fetch https://picsum.photos/id/1043/1920/1280.jpg medium-1920.jpg  # typical hero image
fetch https://picsum.photos/id/1062/1600/1067.webp photo-1600.webp # WebP source
