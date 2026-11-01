# Benchmark

Compares Pixtimize with other **self-hosted** image transform servers on the same machine, using the same source images, the same S3 bucket (MinIO) and the same CPU and memory limits.

| Target | Stack | Reads source from | Caches results |
| --- | --- | --- | --- |
| `pixtimize` | Rust + libvips, built from `../Dockerfile` | S3 | yes: Redis + S3 |
| [`imgproxy`](https://github.com/imgproxy/imgproxy) | Go + libvips | S3 | no |
| [`imagor`](https://github.com/cshum/imagor) | Go + libvips | S3 | yes: S3 result storage |
| [`thumbor`](https://github.com/thumbor/thumbor) | Python + Pillow | HTTP | yes: local disk |
| [`imaginary`](https://github.com/h2non/imaginary) | Go + libvips (no longer actively maintained) | HTTP | no |

Every request asks for a **WebP resized to a given width (height keeps the aspect ratio)** at a given quality. Pixtimize, imgproxy and imagor all use libvips and produce nearly identical files, so their speed comparison is direct.

## Scenarios

- **cold**: every request asks for a transform that hasn't been requested before (a unique width/quality pair), so every server has to fetch, decode, resize and encode the image, and the caching servers also store the result. This compares processing speed.
- **warm**: every request asks for the same transform. Pixtimize, imagor and thumbor answer from their result cache. imgproxy and imaginary have no cache, so they render again; in production they usually sit behind a CDN. This shows what the server does when the same image is requested again.

Each `./run.sh` recreates the whole stack, including volumes, so no cache survives from a previous run.

## Running it

Requirements: Docker with Compose v2, `curl`, `python3`.

```bash
./scripts/fetch-images.sh   # or copy your own .jpg/.png/.webp files into bench/images/
./run.sh
```

The first run builds the Pixtimize image, which takes a few minutes. Results are printed and saved to `results/report.md`, with the raw numbers in `results/*.json`.

Settings (environment variables):

| Variable | Default | Meaning |
| --- | --- | --- |
| `TARGETS` | `pixtimize imgproxy imagor thumbor imaginary` | which servers to run |
| `SCENARIOS` | `cold warm` | |
| `DURATION` | `30s` | length of each run |
| `VUS_COLD` / `VUS_WARM` | `8` / `32` | concurrent clients |
| `BENCH_CPUS` / `BENCH_MEM` | `2` / `2g` | limits applied to each image server |
| `MINIO_IMAGE`, `MC_IMAGE`, `IMGPROXY_IMAGE`, `IMAGOR_IMAGE`, `THUMBOR_IMAGE`, `IMAGINARY_IMAGE`, `K6_IMAGE` | see `docker-compose.yml` | pin a version, or switch if an image can't be pulled |

Example: longer runs, 4 cores per server, Pixtimize against imgproxy and imagor only:

```bash
DURATION=60s BENCH_CPUS=4 TARGETS="pixtimize imgproxy imagor" ./run.sh
```

Stop everything with `docker compose down -v`.

## Reading the results

- Use **ok req/s** (successful responses per second) and **p95/p99** (tail latency) under the same concurrency. Failed requests don't count toward throughput, and an error rate above 1% is flagged with ⚠️. Check that server's logs (`docker compose logs <target>`) before trusting its numbers.
- Check **output KB** (warm runs only, since cold runs mix many widths). A server that produces bigger files at the same quality is doing less compression work, so speed alone doesn't tell the whole story.
- thumbor runs `BENCH_CPUS` Python processes and encodes with Pillow instead of libvips, so it is expected to be much slower. It's included because it is the reference implementation of this URL style.
- Run each benchmark at least twice. Results that differ by less than about 5% between servers are noise.
- The load generator, MinIO and Redis share the host with the server under test. Use a machine with at least `BENCH_CPUS + 2` cores so they don't compete with it.

## Where to run it

- **Dedicated Linux VM or bare metal (best).** For example, a cloud VM with 8 or more dedicated vCPUs in your production region. It's stable, reproducible, and closest to how you'll run Pixtimize. Use this for numbers you publish.
- **Laptop (fine for development).** Plug it in, close other apps, and expect thermal throttling on long runs. On macOS, Docker runs inside a VM, so absolute numbers are lower, but the comparison between servers is still useful.
- **Shared or ephemeral sandboxes (avoid for real numbers).** CPU is shared with other tenants, which makes results noisy.

## Adding another server

Add a service to `docker-compose.yml` (with `<<: *limits`), a URL builder case in `k6/bench.js`, and its name, internal URL and host port in `run.sh`. [weserv/images](https://github.com/weserv/images) is a good next candidate.
