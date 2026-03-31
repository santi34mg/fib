use std::path::Path;
use std::sync::Arc;

use dashmap::DashMap;
use fibc::driver::{CompilationOptions, FrontendResponse};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::definition::goto_definition;
use crate::diagnostics::{analysis_error_to_diagnostic, parse_error_to_diagnostic};
use crate::hover::hover_info;

struct DocumentState {
    result: FrontendResponse,
}

pub struct FiberLanguageServer {
    client: Client,
    documents: DashMap<Url, Arc<DocumentState>>,
}

impl FiberLanguageServer {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: DashMap::new(),
        }
    }

    async fn analyze_and_publish(&self, uri: Url, text: String) {
        let path = uri
            .to_file_path()
            .unwrap_or_else(|_| Path::new("<unknown>").to_path_buf());

        let opts = CompilationOptions {
            project_path: path,
            source: Some(&text),
            include_paths: Vec::new(),
        };
        // Convert to (Option<FrontendResponse>, Option<String>) so the non-Send error is
        // fully dropped before the first await point.
        // FIXME:
        let (frontend_result, frontend_error) = match compile_frontend(opts) {
            Ok(r) => (Some(r), None),
            Err(e) => (None, Some(format!("{}", e))),
        };
        if let Some(msg) = frontend_error {
            self.client
                .publish_diagnostics(
                    uri,
                    vec![Diagnostic {
                        range: Range {
                            start: Position {
                                line: 0,
                                character: 0,
                            },
                            end: Position {
                                line: 0,
                                character: 1,
                            },
                        },
                        severity: Some(DiagnosticSeverity::ERROR),
                        message: msg,
                        source: Some("fiber".into()),
                        ..Default::default()
                    }],
                    None,
                )
                .await;
            return;
        }
        let result = frontend_result.unwrap();

        let mut diagnostics: Vec<Diagnostic> = result
            .parse_errors
            .iter()
            .map(parse_error_to_diagnostic)
            .collect();
        for msg in &result.analysis_errors {
            diagnostics.push(analysis_error_to_diagnostic(msg));
        }

        self.documents
            .insert(uri.clone(), Arc::new(DocumentState { result }));
        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }
}

fn compile_frontend(opts: CompilationOptions) -> Result<FrontendResponse> {
    unimplemented!()
}

#[tower_lsp::async_trait]
impl LanguageServer for FiberLanguageServer {
    async fn initialize(&self, _params: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "fiber-lsp".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
        })
    }

    async fn initialized(&self, _params: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "fiber-lsp initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        self.analyze_and_publish(uri, text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        if let Some(change) = params.content_changes.into_iter().last() {
            self.analyze_and_publish(uri, change.text).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.documents.remove(&params.text_document.uri);
        self.client
            .publish_diagnostics(params.text_document.uri, vec![], None)
            .await;
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let Some(state) = self.documents.get(uri) else {
            return Ok(None);
        };
        Ok(hover_info(
            &state.result,
            pos.line as usize + 1,
            pos.character as usize + 1,
        ))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let Some(state) = self.documents.get(uri) else {
            return Ok(None);
        };

        let location = goto_definition(
            &state.result,
            uri.clone(),
            pos.line as usize + 1,
            pos.character as usize + 1,
        );
        Ok(location.map(GotoDefinitionResponse::Scalar))
    }
}
