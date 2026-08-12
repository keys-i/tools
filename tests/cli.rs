use std::process::Command;

fn command(binary: &str) -> Command {
    let path = match binary {
        "fetch" => env!("CARGO_BIN_EXE_fetch"),
        "games" => env!("CARGO_BIN_EXE_games"),
        _ => unreachable!(),
    };
    let mut command = Command::new(path);
    command.env("NO_COLOR", "1");
    command.env_remove("TOOLS_GAME_PATH");
    command.env(
        "XDG_DATA_HOME",
        std::env::temp_dir().join("keys-tools-test-empty"),
    );
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
fn games_lists_json_and_refuses_unknown_games() {
    let list = command("games")
        .args(["list", "--json"])
        .output()
        .expect("list games");
    assert!(list.status.success());
    let output = String::from_utf8_lossy(&list.stdout);
    assert!(output.starts_with('['));
    assert!(output.contains("\"name\":\"2048\""));

    let missing = command("games")
        .args(["run", "not-a-game"])
        .output()
        .expect("reject missing game");
    assert_eq!(missing.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&missing.stderr).contains("unknown game"));

    let unavailable = command("games")
        .env("PATH", "")
        .args(["run", "2048"])
        .output()
        .expect("report unavailable game");
    assert_eq!(unavailable.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&unavailable.stderr).contains("github.com/ps06756/tui-2048"));
}

#[test]
fn games_launches_a_manifest_command_without_a_shell() {
    let directory = std::env::temp_dir().join(format!(
        "keys-tools-game-pack-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&directory).expect("create pack");
    std::fs::write(
        directory.join("probe.game"),
        format!(
            "name=probe\ncommand={}\nsummary=Test command\nsource=https://example.com/probe\nlicense=MIT\n",
            env!("CARGO_BIN_EXE_fetch")
        ),
    )
    .expect("write manifest");

    let output = command("games")
        .env("TOOLS_GAME_PATH", &directory)
        .args(["run", "probe", "--version"])
        .output()
        .expect("run manifest command");
    let _ = std::fs::remove_dir_all(&directory);

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "fetch 0.1.0"
    );
}
