fn main() {
    let args = fibc::cli::parse_args();

    #[cfg(feature = "llvm")]
    fibc::cli::exec_command(args);

    #[cfg(not(feature = "llvm"))]
    {
        eprintln!("fibc binary built without LLVM support (llvm feature disabled).");
        let _ = args;
        std::process::exit(1);
    }
}
