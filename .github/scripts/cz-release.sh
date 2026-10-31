#!/usr/bin/env bash
# Bumps the version with commitizen (.cz.toml), updates CHANGELOG.md, commits, tags and pushes to main.
# A version that was set by hand and has no tag yet is released as-is instead of bumped.
# Writes version= and released= to $GITHUB_OUTPUT and the release notes to $NOTES.
set -euo pipefail

out="${GITHUB_OUTPUT:-/dev/stdout}"
notes="${NOTES:-notes.md}"

ver=$(cz version -p)
if git rev-parse -q --verify "refs/tags/v$ver" >/dev/null; then
  # 3 = no commits since the tag, 21 = no commits that warrant a bump
  cz --no-raise 3,21 bump --yes
  new=$(cz version -p)
  if [ "$new" = "$ver" ]; then
    echo "Nothing to release since v$ver"
    echo "released=false" >>"$out"
    exit 0
  fi
  ver=$new
else
  cz changelog --unreleased-version "v$ver"
  git add CHANGELOG.md
  git commit -m "bump: release $ver"
  git tag "v$ver"
fi

if [ "${PUSH:-1}" = 1 ]; then
  git push --atomic origin HEAD:main "v$ver"
fi

# the version's changelog section without its heading (the release title has the version)
cz changelog "v$ver" --dry-run | tail -n +2 >"$notes"
echo "version=$ver" >>"$out"
echo "released=true" >>"$out"
