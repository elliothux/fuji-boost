#!/bin/bash
# Rebuild patches/fujimod-devtools.patch from the patched FujiMod checkout,
# including untracked files and the Unicorn submodule, sorted by path.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
src="$root/references/fujimod"
unicorn=crates/emulator-cpu-unicorn/vendor/unicorn
out=$(mktemp)
cd "$src"
{
  git -c core.quotepath=off diff --binary HEAD -- . ":(exclude)$unicorn"
  git ls-files --others --exclude-standard | while read -r f; do git diff --binary --no-index /dev/null "$f" || true; done
  git -C "$unicorn" diff --binary --src-prefix="a/$unicorn/" --dst-prefix="b/$unicorn/"
} | python3 -c '
import re, sys
text = sys.stdin.read()
parts = [p for p in re.split(r"(?m)^(?=diff --git )", text) if p.startswith("diff --git")]
key = lambda p: re.match(r"diff --git \S+ b/(\S+)", p).group(1)
sys.stdout.write("".join(sorted(parts, key=key)))
' > "$out"
mv "$out" "$root/patches/fujimod-devtools.patch"
git apply --reverse --check "$root/patches/fujimod-devtools.patch"
echo "patch rebuilt: $(grep -c '^diff --git' "$root/patches/fujimod-devtools.patch") files"
