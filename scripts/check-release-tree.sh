#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if find "$root" -path "$root/.git" -prune -o \
    \( -name .beads -o -name .claude -o -name .codex -o -name .env -o -name '.env.*' -o -name '*.age' \) \
    -print -quit | grep -q .; then
  printf '%s\n' 'private state/config path found' >&2
  exit 1
fi

if rg -n --hidden --glob '!/.git/**' --glob '!/target/**' \
    --glob '!scripts/check-release-tree.sh' \
    '(/work/repo|/Users/[^/[:space:]]+|/home/[^/[:space:]]+|~/.dotfiles|davidawad@|@users\.noreply\.github\.com)' \
    "$root" >/dev/null; then
  printf '%s\n' 'private path or personal identity reference found' >&2
  exit 1
fi

printf '%s\n' 'release tree path check: clean'
