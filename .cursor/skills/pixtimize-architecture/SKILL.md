---
name: pixtimize-architecture
description: Maintains Pixtimize's layered modular monolith and enforces module boundaries. Use when adding features, moving Rust files, changing handlers or use cases, integrating S3/Redis/libvips, modifying application state, or reviewing the project's architecture.
---

# Pixtimize Architecture

Keep Pixtimize as a single-package, layered modular monolith. Optimize for clear
ownership and navigation rather than maximum abstraction.

## Benefits

- **Faster onboarding**: New contributors can open `application/render_image.rs`
  for the image pipeline, `domain/transform.rs` for ImageKit rules, and
  `infrastructure/*` for S3/Redis/libvips without reading one large handler file.
- **Clear change ownership**: HTTP tweaks stay in `api`, business rules in
  `domain`, SDK details in `infrastructure`, so reviews and diffs stay scoped.
- **Easier testing**: Pure parsing, limits, cache keys, and crop math run in unit
  tests without Axum, live Redis, or S3; integration tests can target the
  assembled router when HTTP contracts matter.
- **Safer refactors**: Swapping a backend (e.g. another object store or cache)
  touches one adapter and wiring in `app.rs`, not handlers and use cases mixed
  together.
- **Stable product behavior**: Use cases encode cache hit/miss, HEAD vs GET, and
  stale-marker recovery in one place, reducing the risk of transport-layer drift.
- **Right-sized complexity**: One Cargo package and concrete adapters avoid
  multi-crate ceremony until multiple binaries, published libraries, or separate
  teams require it.

## Module layout

```text
src/
├── main.rs                  # thin binary entry point
├── lib.rs                   # crate module tree
├── app.rs                   # composition root and shared dependencies
├── config.rs                # environment configuration
├── error.rs                 # application and HTTP error mapping
├── api/                     # Axum transport
├── application/             # use-case orchestration
├── domain/                  # business rules and data structures
└── infrastructure/          # S3, Redis, libvips, scheduler adapters
```

## Layer responsibilities

### `api`

- Extract HTTP paths, queries, methods, and state.
- Convert transport input into application request types.
- Convert application output into HTTP responses.
- Keep handlers thin; do not implement cache, storage, or image workflows here.

### `application`

- Describe complete use cases such as rendering an image or clearing the cache.
- Coordinate domain rules and infrastructure adapters.
- Return technology-neutral result types where practical.
- Own ordering, fallback, and recovery behavior across adapters.

### `domain`

- Own transformation types, parsing rules, limits, cache identity, and metadata.
- Prefer pure synchronous code.
- Do not import Axum, AWS SDK, Redis, Tokio scheduling, or libvips.
- Keep domain types independent of external wire formats except stable
  serialization required for canonical cache keys.

### `infrastructure`

- Wrap each external technology in a named adapter.
- Translate SDK/native errors into `AppError` at the boundary.
- Keep AWS, Redis, libvips, and cron APIs out of other layers.
- Do not orchestrate complete business use cases inside adapters.

### `app`

- Construct adapters and place them in `AppState`.
- Build and run the process.
- Remain the only composition root.
- Do not add business workflows as methods on `AppState`.

## Dependency rules

Use this direction:

```text
main → app → api → application → domain
                         ↓
                  infrastructure
```

- `main.rs` calls `pixtimize::app::run()` only.
- `api` may call `application`, but application must not call API handlers.
- `domain` must not depend on infrastructure.
- Infrastructure adapters may use domain types.
- Cross-layer access goes through explicit public functions or adapter methods.
- Default to `pub(crate)` or private visibility; expose `pub` only at real
  module or crate boundaries.

## Adding a feature

1. Put rules and stable types in `domain`.
2. Put the end-to-end workflow in one `application` use-case module.
3. Add or extend a named adapter in `infrastructure` for external I/O.
4. Add a thin handler and response mapping in `api`.
5. Wire new dependencies in `app.rs`.
6. Add focused unit tests beside pure logic and router-level tests for HTTP
   behavior when useful.

Do not create a directory for a feature until it needs multiple cohesive files.
Split by responsibility, not by an arbitrary line-count limit.

## Adapter guidance

Use concrete adapter types by default:

- `S3Storage`
- `RedisCache`
- `VipsProcessor`

Introduce a trait only when at least one of these is true:

- There are multiple real implementations.
- A use case needs a test double to cover meaningful orchestration.
- A technology boundary is expected to change independently.

Keep traits narrow and owned by the layer that consumes them. Do not build a
generic repository abstraction around an SDK merely for symmetry.

## State rules

`AppState` is a dependency container, not a service:

```rust
state.cache().get(key).await?;
state.storage().get(key).await?;
```

Avoid:

```rust
state.render_and_cache_image(...).await?;
```

The second form hides a use case on shared state and makes ownership difficult
to discover.

## Errors

- Use `thiserror` for structured application errors.
- Use `anyhow` only at process startup and composition boundaries.
- Map external SDK errors inside infrastructure adapters.
- Map errors to status codes in the HTTP boundary.
- Never expose backend error details in 5xx response bodies.
- Avoid `unwrap` and `expect` outside tests or proven-infallible construction.

## Testing

- Keep pure parser, limit, cache-key, and crop tests beside their modules.
- Name tests as behavior: `parse_should_reject_invalid_pair`.
- Test application orchestration with fakes only when a trait seam is justified.
- Test the assembled `Router` from `lib.rs` for important HTTP contracts.
- Do not require live S3 or Redis for ordinary unit tests.

## Documentation

- Use `//!` to explain each module's responsibility.
- Use `///` for public behavior and important invariants.
- Use comments for non-obvious reasons, compatibility constraints, and safety.
- Prefer named functions and types over comments that narrate implementation.
- Update this skill whenever an architectural boundary intentionally changes.

## Validation

After architecture or Rust changes, run:

```shell
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features --locked -- -D warnings
```

Treat failures as incomplete work. Fix warnings instead of broadly suppressing
them.

## Avoid

- Generic `utils`, `helpers`, `services`, or `models` dumping-ground modules.
- HTTP response construction in application use cases.
- AWS, Redis, or libvips calls in handlers.
- Business rules inside infrastructure adapters.
- Methods added to `AppState` from unrelated modules.
- A multi-crate workspace before multiple binaries, reusable crates, or team
  boundaries justify the extra complexity.
