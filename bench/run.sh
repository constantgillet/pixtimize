#!/usr/bin/env bash
# Benchmarks Pixtimize against other image servers. See bench/README.md.
set -euo pipefail
cd "$(dirname "$0")"

TARGETS=${TARGETS:-"pixtimize imgproxy imaginary"}
SCENARIOS=${SCENARIOS:-"cold warm"}
DURATION=${DURATION:-30s}
VUS_COLD=${VUS_COLD:-8}
VUS_WARM=${VUS_WARM:-32}
SEED=${SEED:-$(( $(date +%s) % 72000 ))}

declare -A BASE_URLS=(
  [pixtimize]=http://pixtimize:3000
  [imgproxy]=http://imgproxy:8080
  [imaginary]=http://imaginary:9000
  [pixtimize-remote]=${PIXTIMIZE_REMOTE_URL:-}
  [imagekit]=${IMAGEKIT_URL:-}
)
declare -A HOST_PORTS=([pixtimize]=8081 [imgproxy]=8082 [imaginary]=8083)

[[ -n ${PIXTIMIZE_REMOTE_URL:-} ]] && TARGETS="$TARGETS pixtimize-remote"
[[ -n ${IMAGEKIT_URL:-} ]] && TARGETS="$TARGETS imagekit"

shopt -s nullglob
images=()
for f in images/*.{jpg,jpeg,png,webp}; do images+=("$(basename "$f")"); done
if (( ${#images[@]} == 0 )); then
  echo "No images in bench/images. Run ./scripts/fetch-images.sh or copy your own." >&2
  exit 1
fi

local_targets=()
for t in $TARGETS; do [[ -n ${HOST_PORTS[$t]:-} ]] && local_targets+=("$t"); done

echo "==> Starting stack (${local_targets[*]:-none})"
docker compose up -d --build minio redis "${local_targets[@]}"
docker compose run --rm init >/dev/null

for t in "${local_targets[@]}"; do
  printf "==> Waiting for %s" "$t"
  until [[ $(curl -s -o /dev/null -w '%{http_code}' "http://localhost:${HOST_PORTS[$t]}/") != 000 ]]; do
    printf .; sleep 1
  done
  echo
done

rm -f results/*.json
for scenario in $SCENARIOS; do
  vus=$VUS_COLD; [[ $scenario == warm ]] && vus=$VUS_WARM
  for image in "${images[@]}"; do
    for t in $TARGETS; do
      base=${BASE_URLS[$t]}
      [[ -z $base ]] && continue
      # Make sure Pixtimize really renders on "cold" instead of hitting its cache.
      [[ $t == pixtimize && $scenario == cold ]] && docker compose exec -T redis redis-cli FLUSHALL >/dev/null
      docker compose run --rm -T \
        -e TARGET="$t" -e BASE_URL="$base" -e IMAGE="$image" -e SCENARIO="$scenario" \
        -e VUS="$vus" -e DURATION="$DURATION" -e SEED="$SEED" \
        k6 run --quiet /scripts/bench.js
    done
  done
done

python3 scripts/report.py results > results/report.md
echo
cat results/report.md
echo
echo "Report saved to bench/results/report.md. Stop the stack with: docker compose down"
