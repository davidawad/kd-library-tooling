//! `kb/kb_root.py`: resolver and drift check for THE declarative KB-location
//! primitive (`kb-root.json`, ).
//!
//! CLI (precedence as in the original): `--check` (exit 0 clean / 1 drift),
//! `--json` (whole config, indent 2), `--library`, `--specimen` (:
//! the configured content directory, default `~/KnowledgeBase`), `--corpus` (:
//! the git-tracked text corpus), default = kb_root path.

use crate::py;
use serde_json::Value;
use std::path::{Path, PathBuf};

fn load(kb_dir: &Path) -> Result<Value, String> {
    let path = kb_dir.join("kb-root.json");
    let text = std::fs::read_to_string(&path).map_err(|e| py::oserror(&e, Some(&path)))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

fn field<'a>(cfg: &'a Value, key: &str) -> Result<&'a str, String> {
    cfg.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("kb-root.json: missing string {key}"))
}

/// The canonical knowledge-base root, expanded.
pub fn kb_root(kb_dir: &Path) -> Result<PathBuf, String> {
    Ok(py::expanduser(field(&load(kb_dir)?, "kb_root")?))
}

/// The books/articles library subtree, expanded.
pub fn library_dir(kb_dir: &Path) -> Result<PathBuf, String> {
    Ok(py::expanduser(field(&load(kb_dir)?, "library_dir")?))
}

/// The specimen library (`specimen_dir`, default `~/KnowledgeBase`), expanded
///: one Work folder per held item, original plus chapters.
pub fn specimen_dir(kb_dir: &Path) -> Result<PathBuf, String> {
    let cfg = load(kb_dir)?;
    let raw = cfg
        .get("specimen_dir")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("~/KnowledgeBase");
    Ok(py::expanduser(raw))
}

/// Drift problems (empty = clean).
pub fn check(kb_dir: &Path) -> Result<Vec<String>, String> {
    let mut problems = Vec::new();
    let cfg = load(kb_dir)?;
    let root_raw = field(&cfg, "kb_root")?;

    // Invariant 1: overlays defaults.json agrees on kb_root.
    let defaults = kb_dir.join("overlays").join("defaults.json");
    let overlays_root = std::fs::read_to_string(&defaults)
        .map_err(|e| py::oserror(&e, Some(&defaults)))
        .and_then(|t| serde_json::from_str::<Value>(&t).map_err(|e| e.to_string()))
        .and_then(|v| {
            v.get("kb_root")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| "'kb_root'".to_owned())
        });
    match overlays_root {
        Ok(o) if o != root_raw => problems.push(format!(
            "kb_root drift: kb-root.json says {} but overlays/defaults.json says {} — they must agree",
            py_repr(root_raw),
            py_repr(&o)
        )),
        Ok(_) => {}
        Err(exc) => problems.push(format!("cannot read overlays/defaults.json kb_root: {exc}")),
    }

    // Invariant 2: the root resolves to a real directory.
    let resolved = py::expanduser(root_raw);
    if !resolved.is_dir() {
        problems.push(format!(
            "kb_root does not exist on disk: {}",
            resolved.display()
        ));
    }

    // Invariant 3: the git-tracked text corpus lives under kb_root
    // (directory-boundary comparison, never string prefix). :
    // `corpus_dir` when set; library_dir became the configured content directory
    // (~/KnowledgeBase, outside kb_root), so it is checked only as the legacy
    // fallback for configs without corpus_dir.
    let (key, raw) = match cfg.get("corpus_dir").and_then(Value::as_str) {
        Some(c) if !c.is_empty() => ("corpus_dir", c),
        _ => ("library_dir", field(&cfg, "library_dir")?),
    };
    let lib = py::expanduser(raw);
    if !py::is_relative_to(&lib, &resolved) {
        problems.push(format!(
            "{key} {} is not under kb_root {}",
            lib.display(),
            resolved.display()
        ));
    }

    // Invariant 4: after cutover, kb_root must be a symlink into checkout_dir.
    let repo = cfg.get("knowledge_repo").cloned().unwrap_or(Value::Null);
    if truthy(repo.get("cutover_done")) {
        let checkout = py::expanduser(
            repo.get("checkout_dir")
                .and_then(Value::as_str)
                .ok_or_else(|| "'checkout_dir'".to_owned())?,
        );
        if !resolved.is_symlink() {
            problems.push(format!(
                "cutover_done=true but kb_root is not a symlink (expected symlink -> {})",
                checkout.display()
            ));
        } else if py::realpath(&resolved) != py::realpath(&checkout) {
            problems.push(format!(
                "cutover_done=true but kb_root resolves to {}, not checkout_dir {}",
                py::realpath(&resolved).display(),
                checkout.display()
            ));
        }
    }
    Ok(problems)
}

fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

/// Python `repr(str)` for the strings that reach the drift message.
fn py_repr(s: &str) -> String {
    if s.contains('\'') && !s.contains('"') {
        format!("\"{s}\"")
    } else {
        format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
    }
}

/// `main(argv)`; returns the exit code.
pub fn main(kb_dir: &Path, argv: &[String]) -> i32 {
    let has = |f: &str| argv.iter().any(|a| a == f);
    let fail = |e: String| {
        eprintln!("kb-root: {e}");
        1
    };
    if has("--check") {
        return match check(kb_dir) {
            Err(e) => fail(e),
            Ok(problems) if !problems.is_empty() => {
                for p in problems {
                    eprintln!("kb_root DRIFT: {p}");
                }
                1
            }
            Ok(_) => {
                println!("kb_root: clean");
                0
            }
        };
    }
    if has("--json") {
        return match load(kb_dir) {
            Ok(v) => {
                println!("{}", py::dumps(&v, Some(2)));
                0
            }
            Err(e) => fail(e),
        };
    }
    let path = if has("--library") {
        library_dir(kb_dir)
    } else if has("--specimen") {
        specimen_dir(kb_dir)
    } else if has("--corpus") {
        load(kb_dir).map(|cfg| {
            py::expanduser(
                cfg.get("corpus_dir")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .unwrap_or("~/KnowledgeBase/corpus"),
            )
        })
    } else {
        kb_root(kb_dir)
    };
    match path {
        Ok(p) => {
            println!("{}", p.display());
            0
        }
        Err(e) => fail(e),
    }
}
