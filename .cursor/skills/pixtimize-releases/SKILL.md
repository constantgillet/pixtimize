---
name: pixtimize-releases
description: Enforces Conventional Commits, PR titles, and release-please for Pixtimize. Use when committing, writing PR titles, cutting releases, changing versioning, changelog, commitlint, lefthook, or release-please config.
---

# Pixtimize Commits and Releases

Keep `main` shippable through PRs only. Version bumps and GitHub Releases come
from release-please reading Conventional Commits — not hand-edited versions.

## Branching

`main` is protected: no direct pushes, no force-pushes. Prefer branches named
`feat/…`, `fix/…`, `chore/…`. Merge via pull request.

## Conventional Commits (required)

Commit messages and PR titles must follow
[Conventional Commits](https://www.conventionalcommits.org/) so release-please
can decide version bumps:

| Prefix | Version impact (pre-1.0) | Example |
|--------|--------------------------|---------|
| `fix:` | patch | `fix: correct webp encode path` |
| `feat:` | minor | `feat: add max image size limit` |
| `feat!:` / `BREAKING CHANGE:` | major | `feat!: change cache key format` |
| `chore:`, `docs:`, `ci:`, `refactor:`, `perf:`, `test:` | no release bump by default | `chore: update nixpacks config` |

### Enforcement

- **Local**: Lefthook `commit-msg` hook runs commitlint. After clone, run
  `npm install` once so hooks install (`package.json` `prepare` →
  `lefthook install`). Config: `commitlint.config.js`, `lefthook.yml`.
- **CI**: `.github/workflows/lint-pr.yml` validates PR titles via
  `amannn/action-semantic-pull-request`. The check name is
  `Validate PR title` and is required on `main`.

Do not bypass hooks or invent free-form PR titles; release automation depends
on these prefixes.

In Cursor chat, use `/commit` (`.cursor/commands/commit.md`) to stage and
create a Conventional Commit following commitlint and these rules.

## Automated releases (release-please)

Files:

- `.github/workflows/release-please.yml` — runs on push to `main`
- `release-please-config.json` — `release-type: rust`, tags like `vX.Y.Z`
- `.release-please-manifest.json` — last released version

Flow:

1. Merge feature/fix PRs to `main` with Conventional Commit titles.
2. release-please opens or updates a **Release PR** that bumps `Cargo.toml`,
   updates `CHANGELOG.md`, and refreshes the manifest.
3. Merging that Release PR creates a **git tag** and a **GitHub Release**.

Do not hand-edit the version in `Cargo.toml` for shipping; let the Release PR
own the bump. `bump-minor-pre-major` and `bump-patch-for-minor-pre-major` are
enabled while the project is pre-1.0.

## Tooling note

Node tooling under `package.json` exists only for commitlint/lefthook — it is
not the application runtime. Keep `/node_modules` gitignored.

Architecture boundaries stay in the `pixtimize-architecture` skill; do not put
layering rules here.
