#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  if ! printf '%s' "$haystack" | grep -Fq -- "$needle"; then
    fail "$label: expected output to contain '$needle', got: $haystack"
  fi
}

# 1. Verify hook files exist, are executable, and pass bash syntax check
for hook in pre-commit pre-push; do
  hook_path="$repo_root/.githooks/$hook"
  [ -x "$hook_path" ] || fail "$hook_path must exist and be executable"
  bash -n "$hook_path" || fail "bash -n failed for $hook_path"
done

# 2. End-to-end tests in isolated temporary git repositories
tmp_root="$(mktemp -d "${TMPDIR:-/tmp}/majjj-githooks-test.XXXXXX")"
trap 'rm -rf "$tmp_root"' EXIT

bare_remote="$tmp_root/remote.git"
work_repo="$tmp_root/work"

git init --bare --initial-branch=main "$bare_remote" >/dev/null 2>&1
git init --initial-branch=main "$work_repo" >/dev/null 2>&1
git -C "$work_repo" config user.name "Majjj Test"
git -C "$work_repo" config user.email "test@example.com"
git -C "$work_repo" remote add origin "$bare_remote"

mkdir -p "$work_repo/.githooks" "$work_repo/scripts"
cp "$repo_root/.githooks/pre-commit" "$work_repo/.githooks/pre-commit"
cp "$repo_root/.githooks/pre-push" "$work_repo/.githooks/pre-push"
chmod 755 "$work_repo/.githooks/pre-commit" "$work_repo/.githooks/pre-push"

cat > "$work_repo/scripts/check.sh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" > .stub-check-args
EOF
chmod 755 "$work_repo/scripts/check.sh"

# Seed initial commit before enabling hooks
echo "seed" > "$work_repo/README.md"
git -C "$work_repo" add README.md .githooks scripts/check.sh
git -C "$work_repo" commit -m "seed" >/dev/null 2>&1
git -C "$work_repo" push -u origin main >/dev/null 2>&1

git -C "$work_repo" config core.hooksPath .githooks

# 2a. pre-commit blocks direct commit on main without invoking check.sh
echo "direct-main" >> "$work_repo/README.md"
git -C "$work_repo" add README.md
rm -f "$work_repo/.stub-check-args"
set +e
out="$(git -C "$work_repo" commit -m "should fail" 2>&1)"
rc=$?
set -e
[ "$rc" -ne 0 ] || fail "pre-commit must block direct commits on main"
assert_contains "$out" "Direct commits to 'main' are blocked" "pre-commit main error message"
[ ! -f "$work_repo/.stub-check-args" ] || fail "pre-commit must fail fast on main before running scripts/check.sh"

# 2b. pre-commit allows main commit when MAJJJ_ALLOW_MAIN_COMMIT=1 and forwards to check.sh
MAJJJ_ALLOW_MAIN_COMMIT=1 git -C "$work_repo" commit -m "emergency main commit" >/dev/null 2>&1 \
  || fail "pre-commit must allow commit on main when MAJJJ_ALLOW_MAIN_COMMIT=1"
[ -f "$work_repo/.stub-check-args" ] || fail "pre-commit must run scripts/check.sh when allowed"
assert_contains "$(cat "$work_repo/.stub-check-args")" "--sandbox --staged" "pre-commit check.sh args"

# 2c. pre-push blocks direct push to main
set +e
out="$(git -C "$work_repo" push origin main 2>&1)"
rc=$?
set -e
[ "$rc" -ne 0 ] || fail "pre-push must block direct pushes to main"
assert_contains "$out" "Direct pushes to 'main' are blocked" "pre-push main error message"

# 2d. pre-push allows direct push to main when MAJJJ_ALLOW_MAIN_PUSH=1
MAJJJ_ALLOW_MAIN_PUSH=1 git -C "$work_repo" push origin main >/dev/null 2>&1 \
  || fail "pre-push must allow push to main when MAJJJ_ALLOW_MAIN_PUSH=1"

# 2e. pre-commit and pre-push allow normal feature branches
git -C "$work_repo" checkout -b feat/hook-smoke >/dev/null 2>&1
echo "feature" >> "$work_repo/README.md"
git -C "$work_repo" add README.md
rm -f "$work_repo/.stub-check-args"
git -C "$work_repo" commit -m "feature commit" >/dev/null 2>&1 \
  || fail "pre-commit must allow commits on feature branches"
assert_contains "$(cat "$work_repo/.stub-check-args")" "--sandbox --staged" "feature branch check.sh args"
git -C "$work_repo" push -u origin feat/hook-smoke >/dev/null 2>&1 \
  || fail "pre-push must allow pushes to feature branches"

# 2f. pre-push blocks pushing a feature branch directly into remote main (HEAD:main)
set +e
out="$(git -C "$work_repo" push origin HEAD:main 2>&1)"
rc=$?
set -e
[ "$rc" -ne 0 ] || fail "pre-push must block pushing HEAD:main from a feature branch"
assert_contains "$out" "Direct pushes to 'main' are blocked" "pre-push HEAD:main error message"

# 2g. pre-commit allows detached HEAD (e.g. rebase / bisect)
git -C "$work_repo" checkout --detach HEAD >/dev/null 2>&1
echo "detached" >> "$work_repo/README.md"
git -C "$work_repo" add README.md
rm -f "$work_repo/.stub-check-args"
git -C "$work_repo" commit -m "detached commit" >/dev/null 2>&1 \
  || fail "pre-commit must allow detached HEAD commits"
assert_contains "$(cat "$work_repo/.stub-check-args")" "--sandbox --staged" "detached HEAD check.sh args"

echo "test-githooks: OK"
