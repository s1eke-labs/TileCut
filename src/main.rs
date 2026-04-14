fn main() {
    if let Err(err) = tilecut::run() {
        eprintln!("{}", tilecut::error::render_error(&err));
        std::process::exit(1);
    }
}
