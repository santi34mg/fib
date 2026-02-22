fn main() {
    let args = fiberc::cli::parse_args();
    fiberc::cli::exec_command(args);
}
