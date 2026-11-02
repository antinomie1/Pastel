//! fastfetch reads the WM's name from argv[0]; see `present_as_pastel`.

use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

#[test]
fn compositor_shows_its_proper_name_and_keeps_its_arguments() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_pastel"))
        .args(["--config", "/dev/null"])
        .env("LUMINARIA_BACKEND", "headless")
        .env("PASTEL_EXIT_MS", "10000")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let proc = format!("/proc/{}", child.id());
    let expected = "Pastel\0--config\0/dev/null\0";
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut cmdline = String::new();
    while cmdline != expected && Instant::now() < deadline {
        sleep(Duration::from_millis(10));
        cmdline = std::fs::read_to_string(format!("{proc}/cmdline")).unwrap_or_default();
    }
    let comm = std::fs::read_to_string(format!("{proc}/comm")).unwrap_or_default();
    child.kill().unwrap();
    child.wait().unwrap();
    assert_eq!(cmdline, expected);
    assert_eq!(comm, "pastel\n", "`pkill pastel` must keep working");
}
