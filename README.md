# Pixtimize ⚡🖼️

Pixtimize is an open source image transform API compatible with the ImageKit API. Pixtimize is compatible with any S3 bucket service.

This is a **Rust** rewrite built on **Axum** for a fast, memory-safe web server.

## Tools used

- [Rust](https://www.rust-lang.org/) and [Axum](https://github.com/tokio-rs/axum) as the web framework and runtime
- [AWS SDK for S3](https://crates.io/crates/aws-sdk-s3) to read source images and store transformed ones (any S3-compatible bucket)
- [Redis](https://crates.io/crates/redis) to store the cached image keys
- [`image`](https://crates.io/crates/image) and [`webp`](https://crates.io/crates/webp) for image processing

## How it works

For every request the server:

1. Parses the transform string (from the path or the `tr` query param).
2. Computes a cache key: `sha256(image_path + transformations)`.
3. Looks up the transformed image in the cache. On a `GET` hit the object is fetched from S3; on a `HEAD` hit the body is never downloaded — Redis stores `{s3_key, size, content_type}` and S3 `HeadObject` is used only for legacy markers that lack size.
4. Otherwise it fetches the source from S3, applies the transform, stores the result in S3, records metadata in Redis, and returns it.

A cron job (`CACHE_DELETE_CRON`) periodically clears the cache (Redis markers and the matching S3 objects).

## Transforms compatibility

| Property name | compatible | comment                                                                   |
| ------------- | ---------- | ------------------------------------------------------------------------- |
| w             | ✅         | fixed pixel values, and fractions in `(0, 1)` are treated as a percentage |
| h             | ✅         | fixed pixel values, and fractions in `(0, 1)` are treated as a percentage |
| q             | ✅         | quality of the image, default value is `DEFAULT_QUALITY`                  |
| f             | ✅         | output format, default is `DEFAULT_FORMAT`; values: `jpeg`, `jpg`, `png`, `webp` |

## Limits (ImageKit-compatible)

Aligned with [ImageKit transformation limits](https://imagekit.io/docs/transformations#limits) (free-plan values where adjustable):

| Limit | Value | Behavior |
| ----- | ----- | -------- |
| Max image file size for processing | 20 MB | request rejected |
| Max image megapixels for processing | 25 MP | request rejected |
| Max transform dimensions (`w` / `h`) | 65 535 px | larger absolute values are ignored |
| Max WebP transform / output dimensions | 16 383 px | request rejected |

## Usage

You can transform an image by calling your URL like this (remember to encode the `,` as `%2C` in the query param):

```
https://yourdomain.com/image-example.png?tr=w-606,h-450,f-jpeg
```

or

```
https://yourdomain.com/tr:w-606,h-450,f-jpeg/image-example.png
```

## Configuration

Required environment variables:

- `S3_ENDPOINT`: URL of the S3 bucket (default `https://ams3.digitaloceanspaces.com`)
- `S3_BUCKET`: name of the S3 bucket
- `S3_ACCESS_KEY`: access key id of the S3 bucket
- `S3_SECRET_KEY`: secret key of the S3 bucket
- `REDIS_URL`: Redis connection URL

Optional environment variables:

- `PORT`: port to listen on (default `3000`)
- `S3_REGION`: S3 region (default `ams3`)
- `DEFAULT_QUALITY`: default quality applied to optimize images (default `80`)
- `DEFAULT_FORMAT`: default output format (default `webp`)
- `CACHE_DELETE_CRON`: cron schedule for cache cleanup (default `0 1 * * 1`, every Monday at 1am). Standard 5-field expressions are supported; a seconds field is optional.
- `CACHED_TIME`: `max-age` (in seconds) advertised on served images (default `604800`)

See [`.env.example`](./.env.example) for a template.

## Commands

Install the toolchain from [rustup.rs](https://rustup.rs/), then:

```bash
# run in development
cargo run

# run the tests
cargo test

# build an optimized release binary
cargo build --release
```

## Docker

```bash
docker build -t pixtimize .
docker run --rm -p 3000:3000 --env-file .env pixtimize
```

## Deploy (Nixpacks)

The project ships a [`nixpacks.toml`](./nixpacks.toml) so it can be deployed on any [Nixpacks](https://nixpacks.com/docs/providers/rust)-based platform (Railway, Coolify, Dokploy, Easypanel, ...).

It configures two things the default Rust provider does not handle:

- Extra build packages (`cmake`, `gcc`, `pkg-config`) required to compile `aws-lc-rs` (TLS) and the `webp`/libwebp bindings, plus `cacert` for outbound HTTPS.
- `NIXPACKS_NO_MUSL=1`, because `aws-lc-rs` and libwebp do not build cleanly against the default static musl target.

The server binds to `0.0.0.0:$PORT`, so the platform-provided `PORT` is used automatically. Set the required environment variables (see [Configuration](#configuration)) in your platform's dashboard rather than committing a `.env` file.

Note: many platforms prefer a `Dockerfile` over Nixpacks when both exist. To force the Nixpacks build path, either configure the platform's builder to "Nixpacks" or remove the `Dockerfile`.

## License

Licensed under the [Apache License 2.0](./LICENSE).
