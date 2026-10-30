# Benchmark

Compares Pixtimize with other image transform servers on the same machine, using the same source images and the same CPU and memory limits.

| Target | What it is | Source fetch | Cache |
| --- | --- | --- | --- |
| `pixtimize` | this repo, built from `../Dockerfile` | S3 (MinIO) | Redis + S3 |
| `imgproxy` | [imgproxy](https://github.com/imgproxy/imgproxy) (Go + libvips) | S3 (MinIO) | none |
| `imaginary` | [imaginary](https://github.com/h2non/imaginary) (Go + libvips) | HTTP (MinIO) | none |
| `pixtimize-remote` | optional: your deployed Pixtimize | your bucket | yours |
| `imagekit` | optional: ImageKit (same URL syntax) | ImageKit media library | ImageKit CDN |

Every request asks for a **WebP, resized to a given width**, at a given quality.

## Scenarios

- **cold**: every request asks for a transform that hasn't been requested before (a unique width/quality pair), so every server has to fetch, decode, resize, encode and, for Pixtimize, store the result. This compares processing speed. Redis is flushed before each Pixtimize cold run.
- **warm**: every request asks for the same transform. Pixtimize answers from its cache. imgproxy and imaginary have no cache, so they render again; in production they usually sit behind a CDN. This shows what the origin does when the same image is requested again.

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
| `TARGETS` | `pixtimize imgproxy imaginary` | which local servers to run |
| `SCENARIOS` | `cold warm` | |
| `DURATION` | `30s` | length of each run |
| `VUS_COLD` / `VUS_WARM` | `8` / `32` | concurrent clients |
| `BENCH_CPUS` / `BENCH_MEM` | `2` / `2g` | limits applied to each image server |
| `PIXTIMIZE_REMOTE_URL` | unset | e.g. `https://img.example.com`; adds the `pixtimize-remote` target |
| `IMAGEKIT_URL` | unset | e.g. `https://ik.imagekit.io/your_id`; adds the `imagekit` target |
| `MINIO_IMAGE`, `MC_IMAGE`, `IMGPROXY_IMAGE`, `IMAGINARY_IMAGE`, `K6_IMAGE` | see `docker-compose.yml` | change these if a pinned image can't be pulled |

Example: longer runs, 4 cores per server, imgproxy only:

```bash
DURATION=60s BENCH_CPUS=4 TARGETS="pixtimize imgproxy" ./run.sh
```

Stop everything with `docker compose down`.

### Remote targets (ImageKit, a deployed Pixtimize)

Upload the files from `bench/images/` to the root of your ImageKit media library and/or your Pixtimize bucket, then set `IMAGEKIT_URL` / `PIXTIMIZE_REMOTE_URL`. Both use the ImageKit URL syntax (`/tr:w-800,q-80,f-webp/<image>`).

These numbers include the network and the CDN, so compare remote targets with each other, not with the local ones. Cold runs against ImageKit generate thousands of new transforms, which count against your plan. A short `DURATION` (for example `10s`) keeps that down.

## Reading the results

- Use **req/s** (throughput) and **p95/p99** (tail latency) under the same concurrency.
- Check **avg output KB**. A server that produces bigger files at the same `q` is doing less compression work, so speed alone doesn't tell the whole story.
- Run each benchmark at least twice. Results that differ by less than about 5% between servers are noise.
- The load generator, MinIO and Redis share the host with the server under test. Use a machine with at least `BENCH_CPUS + 2` cores so they don't compete with it.

## Where to run it

- **Dedicated Linux VM or bare metal (best).** For example, a cloud VM with 8 or more dedicated vCPUs in your production region. It's stable, reproducible, and closest to how you'll run Pixtimize. Use this for numbers you publish.
- **Laptop (fine for development).** Plug it in, close other apps, and expect thermal throttling on long runs. On macOS, Docker runs inside a VM, so absolute numbers are lower, but the comparison between servers is still useful.
- **Shared or ephemeral sandboxes (avoid for real numbers).** CPU is shared with other tenants and outbound traffic may go through proxies, which makes results noisy and remote latency meaningless.
