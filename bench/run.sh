#!/usr/bin/env bash
# Benchmarks Pixtimize against other self-hosted image servers. See bench/README.md.
set -euo pipefail
cd "$(dirname "$0")"

TARGETS=${TARGETS:-"pixtimize imgproxy imagor thumbor imaginary"}
SCENARIOS=${SCENARIOS:-"cold warm"}
DURATION=${DURATION:-30s}
VUS_COLD=${VUS_COLD:-8}
VUS_WARM=${VUS_WARM:-32}

# Plain functions instead of associative arrays: macOS ships bash 3.2.
base_url() {
  case $1 in
    pixtimize) echo http://pixtimize:3000 ;;
    imgproxy) echo http://imgproxy:8080 ;;
    imagor) echo http://imagor:8000 ;;
    thumbor) echo http://thumbor:8888 ;;
    imaginary) echo http://imaginary:9000 ;;
  esac
}
host_port() {
  case $1 in
    pixtimize) echo 8081 ;;
    imgproxy) echo 8082 ;;
    imaginary) echo 8083 ;;
    imagor) echo 8084 ;;
    thumbor) echo 8085 ;;
  esac
}

for t in $TARGETS; do
  [[ -n $(base_url "$t") ]] || { echo "Unknown target: $t (known: pixtimize imgproxy imagor thumbor imaginary)" >&2; exit 1; }
done

shopt -s nullglob
images=()
for f in images/*.{jpg,jpeg,png,webp}; do images+=("$(basename "$f")"); done
if (( ${#images[@]} == 0 )); then
  echo "No images in bench/images. Run ./scripts/fetch-images.sh or copy your own." >&2
  exit 1
fi

# Recreate everything (including anonymous volumes) so no cache survives from
# a previous run: every "cold" request is then a real render.
echo "==> Starting fresh stack: $TARGETS"
docker compose up -d --build --force-recreate --renew-anon-volumes s3 redis $TARGETS
docker compose run --rm init >/dev/null

for t in $TARGETS; do
  printf "==> Waiting for %s" "$t"
  until [[ $(curl -s -o /dev/null -w '%{http_code}' "http://localhost:$(host_port "$t")/") != 000 ]]; do
    printf .; sleep 1
  done
  echo
done

rm -f results/*.json
for scenario in $SCENARIOS; do
  vus=$VUS_COLD; [[ $scenario == warm ]] && vus=$VUS_WARM
  for image in "${images[@]}"; do
    for t in $TARGETS; do
      docker compose run --rm -T \
        -e TARGET="$t" -e BASE_URL="$(base_url "$t")" -e IMAGE="$image" -e SCENARIO="$scenario" \
        -e VUS="$vus" -e DURATION="$DURATION" \
        k6 run --quiet /scripts/bench.js 2>&1 \
        | { grep -v 'msg="Request Failed"' || true; }  # errors are counted in the report
    done
  done
done

python3 scripts/report.py results > results/report.md
echo
cat results/report.md
echo
echo "Report saved to bench/results/report.md. Stop the stack with: docker compose down -v"
