# Showcase

The README image is the complete screenshot corpus for this project. It renders real `kb-root` output from the synthetic configuration under `examples/synthetic/kb/`.

Recreate it from the project root:

```sh
docs/showcase/make-screenshots.sh
```

The script runs these exact commands and writes `docs/showcase/kb-root.svg`:

```sh
KB_DIR=examples/synthetic/kb cargo run --quiet --manifest-path Cargo.toml -- --check
KB_DIR=examples/synthetic/kb cargo run --quiet --manifest-path Cargo.toml -- --library
KB_DIR=examples/synthetic/kb cargo run --quiet --manifest-path Cargo.toml -- --corpus
```

The generated SVG is deterministic and stays below 300 KB.
