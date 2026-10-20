//! Small Python-compat helpers so output stays byte-identical to the
//! scripts being replaced (`json.dumps`, `Path.expanduser/resolve`,
//! `OSError` messages, `subprocess.run(timeout=)`).

use serde_json::Value;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn home() -> PathBuf {
    match std::env::var_os("HOME").filter(|h| !h.is_empty()) {
        Some(h) => PathBuf::from(h),
        None => PathBuf::from("/"),
    }
}

/// `os.path.expanduser` for the `~` / `~/...` forms (`~user` is verbatim).
pub fn expanduser(raw: &str) -> PathBuf {
    if raw == "~" {
        home()
    } else if let Some(rest) = raw.strip_prefix("~/") {
        home().join(rest)
    } else {
        PathBuf::from(raw)
    }
}

/// `os.path.realpath` / non-strict `Path.resolve()`: resolve symlinks
/// component by component (dangling links resolve to their target; missing
/// components are kept lexically).
pub fn realpath(path: &Path) -> PathBuf {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    let mut pending: Vec<std::ffi::OsString> = abs
        .components()
        .rev()
        .map(|c| c.as_os_str().to_os_string())
        .collect();
    let mut out = PathBuf::from("/");
    let mut hops = 0;
    while let Some(part) = pending.pop() {
        match Path::new(&part).components().next() {
            Some(Component::RootDir) | Some(Component::CurDir) | None => continue,
            Some(Component::ParentDir) => {
                out.pop();
                continue;
            }
            _ => {}
        }
        let next = out.join(&part);
        match std::fs::read_link(&next) {
            Ok(target) if hops < 40 => {
                hops += 1;
                if target.is_absolute() {
                    out = PathBuf::from("/");
                }
                pending.extend(
                    target
                        .components()
                        .rev()
                        .map(|c| c.as_os_str().to_os_string()),
                );
            }
            _ => out = next,
        }
    }
    out
}

/// `str(json.JSONDecodeError)` for the common "Expecting value" case;
/// other syntax errors fall back to serde's own wording.
pub fn json_error(text: &str, err: &serde_json::Error) -> String {
    let msg = err.to_string();
    if msg.starts_with("expected value")
        || msg.starts_with("expected ident")
        || text.trim().is_empty()
    {
        let pos = text.len() - text.trim_start().len();
        let before = &text[..pos];
        let line = before.matches('\n').count() + 1;
        let col = before.chars().rev().take_while(|c| *c != '\n').count() + 1;
        return format!(
            "Expecting value: line {line} column {col} (char {})",
            before.chars().count()
        );
    }
    msg
}

/// `str(OSError)`: `[Errno N] strerror: 'path'`.
pub fn oserror(err: &io::Error, path: Option<&Path>) -> String {
    let text = err.to_string();
    let msg = match text.rfind(" (os error") {
        Some(i) => &text[..i],
        None => &text,
    };
    match (err.raw_os_error(), path) {
        (Some(n), Some(p)) => format!("[Errno {n}] {msg}: '{}'", p.display()),
        (Some(n), None) => format!("[Errno {n}] {msg}"),
        _ => text,
    }
}

fn escape_str(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            ' '..='~' => out.push(c),
            _ => {
                let mut buf = [0u16; 2];
                for unit in c.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\u{unit:04x}"));
                }
            }
        }
    }
    out.push('"');
}

fn dump(v: &Value, indent: Option<usize>, level: usize, out: &mut String) {
    let nl = |out: &mut String, lvl: usize| {
        if let Some(n) = indent {
            out.push('\n');
            out.push_str(&" ".repeat(n * lvl));
        }
    };
    match v {
        Value::Array(a) if !a.is_empty() => {
            out.push('[');
            for (i, item) in a.iter().enumerate() {
                if i > 0 {
                    out.push_str(if indent.is_some() { "," } else { ", " });
                }
                nl(out, level + 1);
                dump(item, indent, level + 1, out);
            }
            nl(out, level);
            out.push(']');
        }
        Value::Object(m) if !m.is_empty() => {
            out.push('{');
            for (i, (k, item)) in m.iter().enumerate() {
                if i > 0 {
                    out.push_str(if indent.is_some() { "," } else { ", " });
                }
                nl(out, level + 1);
                escape_str(k, out);
                out.push_str(": ");
                dump(item, indent, level + 1, out);
            }
            nl(out, level);
            out.push('}');
        }
        Value::String(s) => escape_str(s, out),
        other => out.push_str(&other.to_string()),
    }
}

/// `json.dumps(v)` (`indent=None`) / `json.dumps(v, indent=n)`, with
/// Python's separators and `ensure_ascii=True`.
pub fn dumps(v: &Value, indent: Option<usize>) -> String {
    let mut out = String::new();
    dump(v, indent, 0, &mut out);
    out
}

/// `Path.is_relative_to` (component-wise, no normalisation of `..`).
pub fn is_relative_to(path: &Path, base: &Path) -> bool {
    path.starts_with(base)
}

/// Why a child process did not yield a normal `CompletedProcess`.
#[derive(Debug)]
pub enum RunError {
    Spawn(io::Error, PathBuf),
    Timeout(Vec<String>, u64),
}

impl RunError {
    /// `str(exc)` for the `(OSError, SubprocessError)` pair the scripts catch.
    pub fn message(&self) -> String {
        match self {
            RunError::Spawn(e, p) => oserror(e, Some(p)),
            RunError::Timeout(argv, secs) => {
                format!(
                    "Command '{}' timed out after {secs} seconds",
                    py_list_repr(argv)
                )
            }
        }
    }
}

fn py_list_repr(argv: &[String]) -> String {
    let items: Vec<String> = argv.iter().map(|a| format!("'{a}'")).collect();
    format!("[{}]", items.join(", "))
}

/// Captured result of [`run_capture`].
pub struct Captured {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// `subprocess.run(argv, capture_output=True, text=True, timeout=secs)`.
/// A signal death reports `-signal` like Python.
pub fn run_capture(argv: &[String], timeout_secs: u64) -> Result<Captured, RunError> {
    use std::os::unix::process::ExitStatusExt;
    let mut child = Command::new(&argv[0])
        .args(&argv[1..])
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| RunError::Spawn(e, PathBuf::from(&argv[0])))?;
    let mut out = child.stdout.take().expect("piped");
    let mut err = child.stderr.take().expect("piped");
    let t_out = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = out.read_to_end(&mut b);
        b
    });
    let t_err = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = err.read_to_end(&mut b);
        b
    });
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(RunError::Timeout(argv.to_vec(), timeout_secs));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(e) => return Err(RunError::Spawn(e, PathBuf::from(&argv[0]))),
        }
    };
    let code = status
        .code()
        .unwrap_or_else(|| -status.signal().unwrap_or(0));
    Ok(Captured {
        code,
        stdout: String::from_utf8_lossy(&t_out.join().unwrap_or_default()).into_owned(),
        stderr: String::from_utf8_lossy(&t_err.join().unwrap_or_default()).into_owned(),
    })
}

/// argparse's `parser.error`: usage + message on stderr, exit 2.
pub fn usage_error(usage: &str, prog: &str, msg: &str) -> i32 {
    eprintln!("{usage}\n{prog}: error: {msg}");
    2
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn dumps_matches_python() {
        let v = json!({"a": [1, 2], "b": {}, "c": "é\u{7f}\"", "d": null, "e": false});
        assert_eq!(
            dumps(&v, None),
            r#"{"a": [1, 2], "b": {}, "c": "\u00e9\u007f\"", "d": null, "e": false}"#
        );
        assert_eq!(
            dumps(&json!({"a": [1], "b": []}), Some(2)),
            "{\n  \"a\": [\n    1\n  ],\n  \"b\": []\n}"
        );
        assert_eq!(dumps(&json!("😀"), None), "\"\\ud83d\\ude00\"");
    }
}
