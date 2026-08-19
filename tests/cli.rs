use std::process::Command;

fn command(binary: &str) -> Command {
    let path = match binary {
        "fetch" => env!("CARGO_BIN_EXE_fetch"),
        "games" => env!("CARGO_BIN_EXE_games"),
        "lazybox" => env!("CARGO_BIN_EXE_lazybox"),
        "science" => env!("CARGO_BIN_EXE_science"),
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

#[test]
fn lazybox_has_a_stable_cli_without_needing_a_backend() {
    let help = command("lazybox")
        .arg("--help")
        .output()
        .expect("show help");
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("lazybox snapshot"));

    let invalid = command("lazybox")
        .args(["snapshot", "--backend", "missing"])
        .output()
        .expect("reject backend");
    assert_eq!(invalid.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("unknown backend"));

    let misplaced = command("lazybox")
        .args(["open", "--plain"])
        .output()
        .expect("reject snapshot format");
    assert_eq!(misplaced.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&misplaced.stderr).contains("snapshot options"));

    let non_tty = command("lazybox")
        .arg("open")
        .output()
        .expect("reject non-terminal dashboard");
    assert_eq!(non_tty.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&non_tty.stderr).contains("needs a terminal"));
}

#[test]
fn science_snapshots_orbits_and_validates_inputs() {
    let orbit = command("science")
        .args(["snapshot", "orbit", "--days", "0", "--json"])
        .output()
        .expect("snapshot orbit");
    assert!(orbit.status.success());
    let output = String::from_utf8_lossy(&orbit.stdout);
    assert!(output.starts_with("{\"view\":\"orbit\""));
    assert_eq!(output.matches("\"distance_au\"").count(), 8);

    let missing = command("science")
        .args(["snapshot", "wave"])
        .output()
        .expect("reject missing VCD");
    assert_eq!(missing.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&missing.stderr).contains("needs a VCD file"));

    let interactive = command("science")
        .args(["open", "orbit"])
        .output()
        .expect("reject non-terminal view");
    assert_eq!(interactive.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&interactive.stderr).contains("need a terminal"));
}
