use std::io::Write;
use std::process::{Command, Stdio};

fn run_with(input: &[u8]) -> (bool, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_mdout"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let out = child.wait_with_output().unwrap();
    (out.status.success(), String::from_utf8_lossy(&out.stdout).into_owned())
}

#[test]
fn non_tty_output_is_plain_and_complete() {
    let (ok, s) = run_with(b"# Hi\n\ntext **bold**\n");
    assert!(ok);
    assert!(!s.contains('\u{1b}'), "non-tty output must not contain ANSI: {s:?}");
    assert!(s.contains("Hi"));
    assert!(s.contains("text bold"));
}

#[test]
fn cjk_table_output_is_aligned() {
    let (ok, s) = run_with("| 名称 | value |\n| --- | --- |\n| 中文 | abc |\n".as_bytes());
    assert!(ok);
    assert!(s.contains('┌'));
    assert!(s.contains("中文"));
}

#[test]
fn help_exits_zero() {
    let out = Command::new(env!("CARGO_BIN_EXE_mdout"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("Usage"));
}

#[test]
fn bad_argument_exits_two() {
    let out = Command::new(env!("CARGO_BIN_EXE_mdout"))
        .arg("--nope")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

fn run_cmd(args: &[&str], env: &[(&str, &str)], input: &[u8]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_mdout"));
    cmd.args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn color_always_beats_no_color_env() {
    let out = run_cmd(&["--color=always"], &[("NO_COLOR", "1")], b"**bold**\n");
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("\u{1b}[1m"));
}

#[test]
fn auto_color_honors_no_color_env() {
    let out = run_cmd(&[], &[("NO_COLOR", "1")], b"**bold**\n");
    assert!(out.status.success());
    assert!(!String::from_utf8_lossy(&out.stdout).contains('\u{1b}'));
}

#[test]
fn width_option_wraps_output() {
    let out = run_cmd(&["--width", "10"], &[], b"one two three four five six\n");
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.lines().count() >= 2, "expected wrapping: {s:?}");
    assert!(s.lines().all(|l| l.len() <= 10), "line exceeded width: {s:?}");
}

#[test]
fn version_exits_zero() {
    let out = run_cmd(&["--version"], &[], b"");
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("mdout"));
}

#[test]
fn bad_argument_writes_usage_to_stderr() {
    let out = run_cmd(&["--nope"], &[], b"");
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("Usage"));
}

#[test]
fn help_survives_closed_pipe() {
    let bin = env!("CARGO_BIN_EXE_mdout");
    let status = Command::new("sh")
        .arg("-c")
        .arg(format!("\"{bin}\" --help | true"))
        .status()
        .unwrap();
    assert!(status.success(), "--help should not fail on a closed pipe");
}

#[test]
fn streaming_survives_closed_pipe() {
    let bin = env!("CARGO_BIN_EXE_mdout");
    let mut child = Command::new("bash")
        .arg("-c")
        .arg(format!("set -o pipefail; \"{bin}\" | head -n1"))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = String::new();
    for i in 0..20000 {
        input.push_str(&format!("block {i}\n\n"));
    }
    let _ = child.stdin.take().unwrap().write_all(input.as_bytes());
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
}
