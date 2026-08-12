fn main() {
    match keys_tools::games::run(std::env::args().skip(1).collect()) {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("games: {error}");
            std::process::exit(2);
        }
    }
}
