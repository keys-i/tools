fn main() {
    match keys_tools::apps::run(std::env::args().skip(1).collect()) {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("apps: {error}");
            std::process::exit(2);
        }
    }
}
