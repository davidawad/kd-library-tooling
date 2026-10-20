#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
project_root="$(cd "$script_dir/../.." && pwd)"
cd "$project_root"

check_output="$(KB_DIR=examples/synthetic/kb cargo run --quiet --manifest-path "$project_root/Cargo.toml" -- --check)"
library_output="$(KB_DIR=examples/synthetic/kb cargo run --quiet --manifest-path "$project_root/Cargo.toml" -- --library)"
corpus_output="$(KB_DIR=examples/synthetic/kb cargo run --quiet --manifest-path "$project_root/Cargo.toml" -- --corpus)"
xml_escape() { sed -e 's/\&/\&amp;/g' -e 's/</\&lt;/g' -e 's/>/\&gt;/g'; }
check_output="$(printf '%s' "$check_output" | xml_escape)"
library_output="$(printf '%s' "$library_output" | xml_escape)"
corpus_output="$(printf '%s' "$corpus_output" | xml_escape)"

cat > "$script_dir/kb-root.svg" <<SVG
<svg xmlns="http://www.w3.org/2000/svg" width="920" height="450" viewBox="0 0 920 450" role="img" aria-labelledby="title desc">
  <title id="title">kb-root knowledge base check</title>
  <desc id="desc">Real kb-root command output using the synthetic example configuration.</desc>
  <rect width="920" height="450" rx="18" fill="#101820"/>
  <rect x="1" y="1" width="918" height="448" rx="18" fill="none" stroke="#34424d"/>
  <circle cx="34" cy="32" r="7" fill="#ef6a62"/><circle cx="58" cy="32" r="7" fill="#e8b94f"/><circle cx="82" cy="32" r="7" fill="#63b87a"/>
  <text x="112" y="39" fill="#aab9c5" font-family="sans-serif" font-size="16">kb-root · synthetic configuration</text>
  <text x="38" y="105" fill="#80cbc4" font-family="monospace" font-size="19">$ KB_DIR=examples/synthetic/kb kb-root --check</text>
  <text x="62" y="139" fill="#e5edf2" font-family="monospace" font-size="19">${check_output}</text>
  <text x="38" y="207" fill="#80cbc4" font-family="monospace" font-size="19">$ KB_DIR=examples/synthetic/kb kb-root --library</text>
  <text x="62" y="241" fill="#e5edf2" font-family="monospace" font-size="19">${library_output}</text>
  <text x="38" y="309" fill="#80cbc4" font-family="monospace" font-size="19">$ KB_DIR=examples/synthetic/kb kb-root --corpus</text>
  <text x="62" y="343" fill="#e5edf2" font-family="monospace" font-size="19">${corpus_output}</text>
  <text x="38" y="411" fill="#8295a3" font-family="sans-serif" font-size="14">All values come from examples/synthetic/kb/kb-root.json.</text>
</svg>
SVG
