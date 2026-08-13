use std::process::Command;

fn command(binary: &str) -> Command {
    let path = match binary {
        "fetch" => env!("CARGO_BIN_EXE_fetch"),
        "games" => env!("CARGO_BIN_EXE_games"),
        _ => unreachable!(),
    };
    let mut command = Command::new(path);
    command.env("NO_COLOR", "1");
    command
}

#[test]
fn fetch_supports_plain_json_and_invalid_input() {
    let plain = command("fetch").arg("--plain").output().expect("run fetch");
    assert!(plain.status.success());
    assert!(String::from_utf8_lossy(&plain.stdout).contains("Host\t"));
    assert!(!plain.stdout.contains(&0x1b));

    let json = command("fetch").arg("--json").output().expect("run fetch");
    assert!(json.status.success());
    assert!(String::from_utf8_lossy(&json.stdout).starts_with("{\"host\":"));

    let invalid = command("fetch").arg("--wat").output().expect("run fetch");
    assert_eq!(invalid.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("unknown option"));
}

#[test]
fn games_lists_native_modes_and_rejects_invalid_input() {
    let list = command("games")
        .args(["list", "--json"])
        .output()
        .expect("list games");
    assert!(list.status.success());
    let output = String::from_utf8_lossy(&list.stdout);
    assert!(output.starts_with('['));
    assert!(output.contains("\"name\":\"merge\""));
    assert!(output.contains("\"name\":\"serpent\""));
    assert_eq!(output.matches("\"native\":true").count(), 11);
    assert!(!output.contains("\"command\""));

    let missing = command("games")
        .args(["run", "not-a-game"])
        .output()
        .expect("reject missing game");
    assert_eq!(missing.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&missing.stderr).contains("unknown game"));

    let invalid_seed = command("games")
        .args(["run", "merge", "--seed", "no"])
        .output()
        .expect("reject invalid seed");
    assert_eq!(invalid_seed.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&invalid_seed.stderr).contains("--seed"));
}

#[test]
fn games_describes_native_modes_and_requires_a_terminal_to_play() {
    let info = command("games")
        .args(["info", "merge"])
        .output()
        .expect("show mode info");
    assert!(info.status.success());
    let text = String::from_utf8_lossy(&info.stdout);
    assert!(text.contains("Implementation\tNative clean-room Rust"));
    assert!(text.contains("Title\tNUMBER MERGE"));

    let non_tty = command("games")
        .args(["run", "merge", "--seed", "1"])
        .output()
        .expect("reject non-terminal play");
    assert_eq!(non_tty.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&non_tty.stderr).contains("need a terminal"));
}
