//! Shared harness: replays frozen Python-oracle scenarios against the Rust
//! binaries. The fixtures under `tests/fixtures/*.json` were recorded once
//! from the retired Python scripts; no Python runs at test time -- failing
//! python/uv/pip/pytest shims lead PATH to prove it.
#![allow(dead_code)]

use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn fixtures(name: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(format!("{name}.json"));
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

/// Directory of shims that fail loudly if anything reaches for Python tooling.
pub fn failing_shims(dir: &Path) -> PathBuf {
    let shims = dir.join("shims");
    fs::create_dir_all(&shims).unwrap();
    for name in ["python3", "python", "uv", "uvx", "pip", "pip3", "pytest"] {
        let p = shims.join(name);
        fs::write(
            &p,
            format!("#!/bin/sh\necho \"FORBIDDEN: {name} invoked\" >&2\nexit 97\n"),
        )
        .unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
    }
    shims
}

pub fn sub(s: &str, tmp: &str) -> String {
    s.replace("{TMP}", tmp)
}

fn sub_value(v: &Value, tmp: &str) -> Value {
    match v {
        Value::String(s) => Value::String(sub(s, tmp)),
        Value::Array(a) => Value::Array(a.iter().map(|x| sub_value(x, tmp)).collect()),
        other => other.clone(),
    }
}

pub fn materialize(spec: &Value, tmp: &Path) {
    let t = tmp.to_str().unwrap();
    if let Some(dirs) = spec.get("dirs").and_then(Value::as_array) {
        for d in dirs {
            fs::create_dir_all(tmp.join(sub(d.as_str().unwrap(), t))).unwrap();
        }
    }
    if let Some(files) = spec.get("files").and_then(Value::as_object) {
        for (rel, c) in files {
            let p = tmp.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            let (content, exec) = match c {
                Value::String(s) => (s.as_str(), false),
                o => (
                    o["content"].as_str().unwrap(),
                    o["exec"].as_bool().unwrap_or(false),
                ),
            };
            fs::write(&p, sub(content, t)).unwrap();
            if exec {
                fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
            }
        }
    }
    if let Some(links) = spec.get("symlinks").and_then(Value::as_object) {
        for (rel, target) in links {
            let p = tmp.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::os::unix::fs::symlink(sub(target.as_str().unwrap(), t), &p).unwrap();
        }
    }
}

fn norm(s: &str, tmp: &str) -> String {
    let s = s.replace(tmp, "{TMP}");
    // ISO-8601 UTC timestamps -> <TS>
    let bytes: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        let rest: String = bytes[i..].iter().take(40).collect();
        if let Some(len) = ts_len(&rest) {
            out.push_str("<TS>");
            i += len;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}

fn ts_len(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    let shape = b"dddd-dd-ddTdd:dd:dd";
    if b.len() < shape.len() {
        return None;
    }
    for (i, c) in shape.iter().enumerate() {
        let ok = if *c == b'd' {
            b[i].is_ascii_digit()
        } else {
            b[i] == *c
        };
        if !ok {
            return None;
        }
    }
    let mut n = shape.len();
    if b.get(n) == Some(&b'.') {
        n += 1;
        while b.get(n).is_some_and(u8::is_ascii_digit) {
            n += 1;
        }
    }
    if b.get(n..n + 6) == Some(b"+00:00") {
        Some(n + 6)
    } else {
        None
    }
}

fn walk(base: &Path, out: &mut Vec<PathBuf>) {
    let mut kids: Vec<PathBuf> = fs::read_dir(base)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    kids.sort();
    for k in kids {
        out.push(k.clone());
        if k.is_dir() && !k.is_symlink() {
            walk(&k, out);
        }
    }
}

fn tree(tmp: &Path, rels: &[Value]) -> Value {
    let t = tmp.to_str().unwrap();
    let mut out = serde_json::Map::new();
    for rel in rels {
        let rel = rel.as_str().unwrap();
        let base = tmp.join(rel);
        if fs::symlink_metadata(&base).is_err() {
            out.insert(rel.into(), Value::Null);
            continue;
        }
        let mut paths = vec![];
        if base.is_dir() && !base.is_symlink() {
            walk(&base, &mut paths);
        } else {
            paths.push(base.clone());
        }
        for p in paths {
            let r = p.strip_prefix(tmp).unwrap().to_str().unwrap().to_owned();
            let v = if p.is_symlink() {
                format!(
                    "-> {}",
                    fs::read_link(&p)
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .replace(t, "{TMP}")
                )
            } else if p.is_file() {
                norm(&fs::read_to_string(&p).unwrap(), t)
            } else {
                "<dir>".to_owned()
            };
            out.insert(r, Value::String(v));
        }
    }
    Value::Object(out)
}

/// Replay every scenario in `fixtures/<name>.json` against `bin`.
pub fn replay(name: &str, bin: &str) {
    let mut failures = Vec::new();
    for case in fixtures(name) {
        let spec = &case["spec"];
        let exp = &case["expected"];
        let label = spec["name"].as_str().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let tmp = fs::canonicalize(dir.path()).unwrap();
        let t = tmp.to_str().unwrap();
        materialize(spec, &tmp);
        fs::create_dir_all(tmp.join("synthetic-home")).unwrap();
        let shims = failing_shims(&tmp);
        let kb_dir = tmp.join(spec.get("kb_dir").and_then(Value::as_str).unwrap_or("kb"));
        let argv: Vec<String> = spec["argv"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| sub(a.as_str().unwrap(), t))
            .collect();
        let mut cmd = Command::new(bin);
        cmd.args(&argv)
            .current_dir(&tmp)
            .env_clear()
            .env("PATH", format!("{}:{t}/bin:/usr/bin:/bin", shims.display()))
            .env("HOME", format!("{t}/synthetic-home"))
            .env("KB_DIR", &kb_dir)
            .env("TMPDIR", t);
        if let Some(env) = spec.get("env").and_then(Value::as_object) {
            for (k, v) in sub_value(&Value::Object(env.clone()), t)
                .as_object()
                .unwrap()
            {
                cmd.env(k, v.as_str().unwrap());
            }
        }
        let out = cmd.output().unwrap();
        let got_stdout = norm(&String::from_utf8_lossy(&out.stdout), t);
        let got_stderr = norm(&String::from_utf8_lossy(&out.stderr), t);
        let got_tree = tree(
            &tmp,
            spec.get("tree")
                .and_then(Value::as_array)
                .map_or(&[][..], Vec::as_slice),
        );
        let mut bad = Vec::new();
        if i64::from(out.status.code().unwrap_or(-1)) != exp["code"].as_i64().unwrap() {
            bad.push(format!("code {:?} != {}", out.status.code(), exp["code"]));
        }
        if got_stdout != exp["stdout"].as_str().unwrap() {
            bad.push(format!(
                "stdout:\n got: {got_stdout:?}\n exp: {:?}",
                exp["stdout"]
            ));
        }
        if let Some(es) = exp["stderr"].as_str() {
            if got_stderr != es {
                bad.push(format!("stderr:\n got: {got_stderr:?}\n exp: {es:?}"));
            }
        }
        if got_tree != exp["tree"] {
            bad.push(format!("tree:\n got: {got_tree}\n exp: {}", exp["tree"]));
        }
        assert!(
            !got_stderr.contains("FORBIDDEN"),
            "{label}: Python tooling invoked"
        );
        if !bad.is_empty() {
            failures.push(format!("[{name}/{label}]\n{}", bad.join("\n")));
        }
    }
    assert!(
        failures.is_empty(),
        "{} scenario(s) diverged from the frozen oracle:\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}
