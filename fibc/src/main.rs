fn main() {
    let args = fibc::cli::parse_args();

    #[cfg(feature = "llvm")]
    fibc::cli::exec_command(args);
}
