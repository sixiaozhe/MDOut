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
