Create a git commit for the current changes using this repo's Conventional Commits rules.

## Steps

1. Run in parallel:
   - `git status`
   - `git diff` and `git diff --staged`
   - `git log -8 --oneline` (match message style)
2. Draft a **Conventional Commit** subject (≤ ~72 chars), focus on why:
   - `feat:` new user-facing capability → minor bump (pre-1.0)
   - `fix:` bug fix → patch bump
   - `feat!:` / body `BREAKING CHANGE:` → major bump
   - `chore:`, `docs:`, `ci:`, `refactor:`, `perf:`, `test:`, `build:`, `style:` when no release bump is intended
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
6. If commitlint/lefthook rejects the message, fix the message and create a **new** commit (do not `--amend` unless the amend rules below all apply).
7. Do **not** push unless the user asked to push.

## Rules

- Follow the project's commitlint config (`commitlint.config.js`) and architecture skill commit guidance.
- Never update git config, never `--no-verify`, never force-push.
- Do not amend unless the user asked, HEAD was created by you in this conversation, and the commit is not pushed.
- If the user added extra context after `/commit`, incorporate it into the message when accurate.
- If there is nothing to commit, say so and stop.
