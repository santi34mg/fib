# fibc-fib

This is a handwritten experiment toward implementing the Fib compiler in Fib,
not a working compiler or bootstrap path. It currently contains token records,
an arena type and a command skeleton; tokenization, parsing and compilation are
not implemented.

It is outside the top-level sample execution sweep and has not been validated
against current syntax and library APIs; its command dispatch uses
`std::string::eq`. The Rust workspace
remains the compiler implementation.
