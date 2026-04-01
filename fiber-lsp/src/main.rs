mod backend;
mod definition;
mod diagnostics;
mod hover;
mod lookup;

use backend::FiberLanguageServer;
use tower_lsp::{LspService, Server};

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(FiberLanguageServer::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
