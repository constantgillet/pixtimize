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

## Creating a commit (`/commit`)

When the user runs `/commit` or asks to commit, follow this procedure.
`.cursor/commands/commit.md` only points here — keep the steps in this skill.

1. Run in parallel:
   - `git status`
   - `git diff` and `git diff --staged`
   - `git log -8 --oneline` (match message style)
2. Draft a Conventional Commit subject (≤ ~72 chars), focus on why. Use the
   prefix table above (`feat:`, `fix:`, `chore:`, …).
3. Stage relevant files only (never `.env`, credentials, or secrets).
4. Commit with a HEREDOC:

```bash
git commit -m "$(cat <<'EOF'
type(scope): short summary

Optional body if needed.

EOF
)"
```

5. Run `git status` after the commit.
6. If commitlint/lefthook rejects the message, fix it and create a **new**
   commit (do not `--amend` unless the amend rules below all apply).
7. Do **not** push unless the user asked to push.

Rules:

- Obey `commitlint.config.js` and this skill.
- Never update git config, never `--no-verify`, never force-push.
- Do not amend unless the user asked, HEAD was created by you in this
  conversation, and the commit is not pushed.
- If the user added extra context after `/commit`, incorporate it when accurate.
- If there is nothing to commit, say so and stop.

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
