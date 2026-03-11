fn main() {
    let args = fibc::cli::parse_args();
    fibc::cli::exec_command(args);
}
