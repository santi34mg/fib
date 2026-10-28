//! Editor services built from the same tokens, syntax and semantic checks as compilation.
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
};

use super::{
    analysis_annotation::{BindingAnnotation, MemberKind, NameAnnotation},
    analyzer::{AnalysisReport, Analyzer, walk},
    ast::{
        expression::{Expression, ExpressionKind},
        qualified_name::QualifiedIdentifier,
    },
    identifier::{Entity, ResolvedName},
    parser::{ParseError, Parser},
    semantic_type::{SemanticType, SemanticTypes},
    source::SourceProvider,
    tokenizer::Tokenizer,
    tokens::{Literal, Token, TokenKind, builtin::BuiltinType},
};
use crate::{
    diagnostics::{CompilerError, ErrorKind, Span},
    driver::CompilationUnit,
    plan::{BuildPlan, FileId, FileName, SourceFile},
};

pub const TOKEN_TYPES: &[&str] = &[
    "namespace",
    "type",
    "parameter",
    "variable",
    "property",
    "enumMember",
    "function",
    "keyword",
    "comment",
    "string",
    "number",
    "operator",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

/// Converts the compiler's one-based Unicode scalar columns to LSP UTF-16.
pub struct SourceIndex<'a> {
    lines: Vec<&'a str>,
}
impl<'a> SourceIndex<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            lines: source
                .split('\n')
                .map(|l| l.strip_suffix('\r').unwrap_or(l))
                .collect(),
        }
    }
    pub fn position(&self, span: Span) -> Position {
        let line = span.line.saturating_sub(1).min(self.lines.len() - 1);
        Position {
            line: line as u32,
            character: self.lines[line]
                .chars()
                .take(span.column.saturating_sub(1))
                .map(char::len_utf16)
                .sum::<usize>() as u32,
        }
    }
    pub fn token_range(&self, token: &Token) -> Range {
        Range {
            start: self.position(Span::new(token.line, token.column)),
            end: self.position(Span::new(token.end_line, token.end_column + 1)),
        }
    }
    pub fn diagnostic_range(&self, diagnostic: &CompilerError, tokens: &[Token]) -> Range {
        let Some(span) = diagnostic.span else {
            return Range::default();
        };
        if let Some(token) = tokens.iter().find(|t| {
            (t.line, t.column) <= (span.line, span.column)
                && (t.end_line, t.end_column) >= (span.line, span.column)
        }) {
            return self.token_range(token);
        }
        let start = self.position(span);
        let end = self.position(Span::new(span.line, span.column.saturating_add(1)));
        Range { start, end }
    }
}

#[derive(Debug)]
pub struct EditorDiagnostic {
    pub diagnostic: CompilerError,
    pub warning: bool,
}
#[derive(Debug)]
pub struct DocumentAnalysis {
    pub source: String,
    pub tokens: Vec<Token>,
    pub semantic_tokens: Vec<u32>,
    pub diagnostics: Vec<EditorDiagnostic>,
}
#[derive(Default, Debug)]
pub struct ProjectAnalysis {
    pub documents: BTreeMap<PathBuf, DocumentAnalysis>,
}

impl ProjectAnalysis {
    pub fn add_error(&mut self, path: &Path, error: anyhow::Error, sources: &dyn SourceProvider) {
        let diagnostic = compiler_error(error, path);
        let path = diagnostic
            .filename
            .clone()
            .unwrap_or_else(|| path.to_owned());
        let document = self.documents.entry(path.clone()).or_insert_with(|| {
            let source = sources.read(&path).unwrap_or_default();
            let tokens = Tokenizer::new(&source).collect::<Vec<_>>();
            DocumentAnalysis {
                semantic_tokens: encode_tokens(&source, &tokens, &HashMap::new()),
                source,
                tokens,
                diagnostics: Vec::new(),
            }
        });
        document.diagnostics.push(EditorDiagnostic {
            diagnostic,
            warning: false,
        });
    }
    pub fn merge(&mut self, other: Self) {
        self.documents.extend(other.documents);
    }
}

pub fn compiler_error(error: anyhow::Error, path: &Path) -> CompilerError {
    let mut diagnostic = if let Some(error) = error.downcast_ref::<CompilerError>() {
        error.clone()
    } else if let Some(error) = error.downcast_ref::<ParseError>() {
        CompilerError::new(ErrorKind::Parse, error.message.clone())
            .at(error.line, error.column)
            .with_source(error.filename.to_path_buf(), error.source_line.clone())
    } else {
        let kind = if error.downcast_ref::<std::io::Error>().is_some() {
            ErrorKind::Io
        } else {
            ErrorKind::Type
        };
        CompilerError::new(kind, format!("{error:#}"))
    };
    if diagnostic.filename.is_none() {
        diagnostic.filename = Some(path.to_owned());
    }
    diagnostic
}

/// A standalone editor document need not exist on disk or define an entry function.
pub fn standalone_plan(path: &Path) -> BuildPlan {
    BuildPlan {
        standard_library: crate::standard_library::StandardLibrary::default(),
        native: crate::plan::NativeInputs::default(),
        packages: vec![],
        files: vec![SourceFile {
            id: FileId(0),
            name: FileName(
                path.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            ),
            path: path.to_owned(),
        }],
        entry: FileId(0),
        output: path.with_extension("lsp-unused"),
    }
}

/// No lowering, executable entry validation or source discovery is performed here.
pub fn analyze_plan(
    plan: &BuildPlan,
    sources: &dyn SourceProvider,
    build_file: bool,
) -> ProjectAnalysis {
    let mut output = ProjectAnalysis::default();
    let prepared = match plan.prepare() {
        Ok(plan) => plan,
        Err(error) => {
            if let Some(file) = plan.files.iter().find(|f| f.id == plan.entry) {
                output.add_error(&file.path, error, sources);
            }
            return output;
        }
    };
    let plan = &prepared;
    let mut units = Vec::new();
    let mut roles = HashMap::<FileId, HashMap<Span, &'static str>>::new();
    let mut unavailable = std::collections::HashSet::new();
    for file in &plan.files {
        let source = match sources.read(&file.path) {
            Ok(source) => source,
            Err(error) => {
                output.add_error(&file.path, error.into(), sources);
                unavailable.insert(file.id);
                continue;
            }
        };
        let tokens = Tokenizer::new(&source).collect::<Vec<_>>();
        let mut parser = Parser::with_source(tokens.clone().into_iter(), &file.path, &source);
        let parsed = parser.parse();
        let syntax_roles = parser.name_sites.into_iter().collect::<HashMap<_, _>>();
        let mut diagnostics = Vec::new();
        match parsed {
            Ok(ast) => units.push(CompilationUnit {
                source_file_id: file.id,
                ast,
            }),
            Err(error) => {
                unavailable.insert(file.id);
                diagnostics.push(EditorDiagnostic {
                    diagnostic: compiler_error(error, &file.path),
                    warning: false,
                });
            }
        }
        let semantic_tokens = encode_tokens(&source, &tokens, &syntax_roles);
        output.documents.insert(
            file.path.clone(),
            DocumentAnalysis {
                source,
                tokens,
                semantic_tokens,
                diagnostics,
            },
        );
        roles.insert(file.id, syntax_roles);
    }
    let report = match Analyzer::analyze_available(plan.clone(), units, unavailable, build_file) {
        Ok(report) => report,
        Err(error) => {
            if let Some(file) = plan.files.first() {
                output.add_error(&file.path, error, sources);
            }
            return output;
        }
    };
    apply_report(&mut output, plan, report, roles);
    output
}

fn apply_report(
    output: &mut ProjectAnalysis,
    plan: &BuildPlan,
    mut report: AnalysisReport,
    mut roles: HashMap<FileId, HashMap<Span, &'static str>>,
) {
    for unit in &mut report.units {
        let Some(file) = plan.files.iter().find(|f| f.id == unit.source_file_id) else {
            continue;
        };
        let Some(document) = output.documents.get_mut(&file.path) else {
            continue;
        };
        let mut classifier = Classifier {
            roles: roles.entry(file.id).or_default(),
            tokens: &document.tokens,
            types: &report.types,
            symbols: &report.symbols,
            parameters: std::collections::HashSet::new(),
        };
        // The structural walker visits syntax even when semantic slots are pending.
        let _ = walk::statements(&mut classifier, &mut unit.ast.statements, true);
        document.semantic_tokens =
            encode_tokens(&document.source, &document.tokens, classifier.roles);
    }
    for (diagnostic, warning) in report
        .errors
        .into_iter()
        .map(|d| (d, false))
        .chain(report.warnings.into_iter().map(|w| (w.diagnostic, true)))
    {
        if let Some(document) = diagnostic
            .filename
            .as_ref()
            .and_then(|p| output.documents.get_mut(p))
        {
            document.diagnostics.push(EditorDiagnostic {
                diagnostic,
                warning,
            });
        }
    }
}

struct Classifier<'a> {
    roles: &'a mut HashMap<Span, &'static str>,
    tokens: &'a [Token],
    types: &'a SemanticTypes,
    symbols: &'a super::namespace::ProjectSymbols,
    parameters: std::collections::HashSet<super::analysis_annotation::BindingId>,
}
impl Classifier<'_> {
    fn info_role(&self, info: super::analysis_annotation::ExpressionInfo) -> &'static str {
        if info.type_value.is_some() {
            "type"
        } else if matches!(
            self.types
                .canonical(info.ty)
                .and_then(|ty| self.types.get(ty)),
            Ok(SemanticType::Function(_) | SemanticType::IntrinsicFunction)
        ) {
            "function"
        } else {
            "variable"
        }
    }
    fn name_role(&self, annotation: NameAnnotation) -> Option<&'static str> {
        let name = annotation.resolved?;
        let binding = match name {
            ResolvedName::Entity(Entity::File(_)) => return Some("namespace"),
            ResolvedName::Entity(Entity::Declaration(id)) => {
                super::analysis_annotation::BindingId::Declaration(id)
            }
            ResolvedName::Local(id) => super::analysis_annotation::BindingId::Local(id),
        };
        self.types.bindings.get(&binding).map(|info| {
            if self.parameters.contains(&binding) && info.type_value.is_none() {
                "parameter"
            } else {
                self.info_role(*info)
            }
        })
    }
}
impl walk::Visitor for Classifier<'_> {
    fn name(&mut self, name: &mut QualifiedIdentifier) -> anyhow::Result<()> {
        let Some(start) = name.annotation.source_span.map(Span::from) else {
            return Ok(());
        };
        let final_role = self.name_role(name.annotation);
        if let Some(index) = self
            .tokens
            .iter()
            .position(|t| t.line == start.line && t.column == start.column)
        {
            for (offset, _) in name.segments.iter().enumerate() {
                // Comments may occur between the parser's qualified-name segments.
                if let Some(token) = self.tokens[index..]
                    .iter()
                    .filter(|t| matches!(t.kind, TokenKind::Identifier(_)))
                    .nth(offset)
                {
                    let role = if offset + 1 < name.segments.len() {
                        Some("namespace")
                    } else {
                        final_role
                    };
                    if let Some(role) = role {
                        self.roles.insert(Span::new(token.line, token.column), role);
                    }
                }
            }
        }
        Ok(())
    }
    fn binding(
        &mut self,
        _name: &str,
        slot: &mut BindingAnnotation,
        _span: Span,
    ) -> anyhow::Result<()> {
        if let (Some(span), Some(binding)) = (slot.source_span.map(Span::from), slot.binding) {
            if self.roles.get(&span) == Some(&"parameter") {
                self.parameters.insert(binding);
            }
            let namespace = matches!(binding, super::analysis_annotation::BindingId::Declaration(id) if matches!(self.symbols.canonical(Entity::Declaration(id)), Ok(Entity::File(_))));
            let role = if namespace {
                Some("namespace")
            } else {
                self.types.bindings.get(&binding).map(|info| {
                    if self.parameters.contains(&binding) && info.type_value.is_none() {
                        "parameter"
                    } else {
                        self.info_role(*info)
                    }
                })
            };
            if let Some(role) = role {
                self.roles.insert(span, role);
            }
        }
        Ok(())
    }
    fn expression(&mut self, node: &mut Expression) -> anyhow::Result<()> {
        if let ExpressionKind::ExprAcess { member, .. } = &node.kind
            && let (Some(span), Some(info)) = (member.source_span.map(Span::from), member.resolved)
        {
            self.roles.insert(
                span,
                if matches!(info.kind, MemberKind::EnumVariant(_)) {
                    "enumMember"
                } else {
                    "property"
                },
            );
        }
        Ok(())
    }
}

pub fn encode_tokens(
    source: &str,
    tokens: &[Token],
    roles: &HashMap<Span, &'static str>,
) -> Vec<u32> {
    let index = SourceIndex::new(source);
    let mut data = Vec::new();
    let mut previous = Position::default();
    for token in tokens {
        let role = match &token.kind {
            TokenKind::Identifier(name) => roles
                .get(&Span::new(token.line, token.column))
                .copied()
                .unwrap_or_else(|| {
                    if BuiltinType::from_name(name).is_some() {
                        "type"
                    } else {
                        "variable"
                    }
                }),
            TokenKind::Keyword(_) => "keyword",
            TokenKind::Comment => "comment",
            TokenKind::Literal(
                Literal::String(_) | Literal::CString(_) | Literal::Character(_),
            ) => "string",
            TokenKind::Literal(Literal::Integer(_) | Literal::Float(_)) => "number",
            TokenKind::Literal(_) => "keyword",
            TokenKind::Operator(_) => "operator",
            _ => continue,
        };
        let range = index.token_range(token);
        for line in range.start.line..=range.end.line {
            let start = if line == range.start.line {
                range.start.character
            } else {
                0
            };
            let end = if line == range.end.line {
                range.end.character
            } else {
                index.lines[line as usize].encode_utf16().count() as u32
            };
            if end <= start {
                continue;
            }
            let delta_line = line - previous.line;
            data.extend([
                delta_line,
                if delta_line == 0 {
                    start - previous.character
                } else {
                    start
                },
                end - start,
                TOKEN_TYPES.iter().position(|r| *r == role).unwrap() as u32,
                0,
            ]);
            previous = Position {
                line,
                character: start,
            };
        }
    }
    data
}

#[cfg(test)]
mod tests {
    fn standalone_plan(path: &std::path::Path) -> crate::plan::BuildPlan {
        super::standalone_plan(path)
            .with_standard_library(crate::standard_library::StandardLibrary::disabled())
    }

    use super::super::source::OverlaySources;
    use super::*;

    fn analyze(source: &str) -> DocumentAnalysis {
        let path = PathBuf::from("/editor/main.fib");
        let sources = OverlaySources([(path.clone(), source.to_owned())].into_iter().collect());
        analyze_plan(&standalone_plan(&path), &sources, false)
            .documents
            .remove(&path)
            .unwrap()
    }
    fn messages(document: &DocumentAnalysis) -> Vec<&str> {
        document
            .diagnostics
            .iter()
            .map(|d| d.diagnostic.message.as_str())
            .collect()
    }

    #[test]
    fn collects_independent_names_types_and_flow_in_one_snapshot() {
        let document = analyze(
            "fn names() { missing; another; }\nfn types() { a: bool = 1; b: int32 = false; }\nfn flow() int32 { x: int32; y: int32; x; y; }",
        );
        let messages = messages(&document);
        assert!(
            messages.iter().any(|m| m.contains("missing")),
            "{messages:?}"
        );
        assert!(
            messages.iter().any(|m| m.contains("another")),
            "{messages:?}"
        );
        assert!(messages.len() >= 6, "{messages:?}");
        assert!(
            !messages.iter().any(|m| m.contains("pending")),
            "{messages:?}"
        );
    }
    #[test]
    fn failed_binding_blocks_dependents_and_keeps_unrelated_errors() {
        let document = analyze("fn f() { x := unknown; x; y: bool = 1; }");
        let messages = messages(&document);
        assert_eq!(messages.len(), 2, "{messages:?}");
        assert!(messages.iter().any(|m| m.contains("unknown")));
        assert!(
            !messages
                .iter()
                .any(|m| m.contains("missing type") || m.contains("pending"))
        );
    }
    #[test]
    fn collects_duplicate_declarations_without_poisoning_original() {
        let document = analyze("fn first() {}\nfn first() {}\nfn second() { bad; }");
        assert_eq!(document.diagnostics.len(), 2, "{:?}", messages(&document));
    }
    #[test]
    fn valid_program_agrees_with_strict_analysis() {
        let source = "comp Pair := struct { x: int32 }; fn id(T: type, x: T) T { return x; } fn main() int32 { return id(int32, 4); }";
        let document = analyze(source);
        assert!(document.diagnostics.is_empty(), "{:?}", messages(&document));
        let path = PathBuf::from("/editor/main.fib");
        let ast = Parser::with_source(Tokenizer::new(source), &path, source)
            .parse()
            .unwrap();
        Analyzer::new(
            standalone_plan(&path),
            vec![CompilationUnit {
                source_file_id: FileId(0),
                ast,
            }],
        )
        .unwrap()
        .analyze()
        .unwrap();
    }
    #[test]
    fn numeric_and_predefined_names_follow_compiler_tokens() {
        let document = analyze("comp T := struct { n: int32 }; fn f(x: T) int32 { return x.n; }");
        assert!(document.diagnostics.is_empty(), "{:?}", messages(&document));
        assert!(
            document
                .semantic_tokens
                .as_chunks::<5>()
                .0
                .iter()
                .any(|t| TOKEN_TYPES[t[3] as usize] == "type")
        );
        assert!(
            document
                .semantic_tokens
                .as_chunks::<5>()
                .0
                .iter()
                .any(|t| TOKEN_TYPES[t[3] as usize] == "parameter")
        );
        assert!(
            document
                .semantic_tokens
                .as_chunks::<5>()
                .0
                .iter()
                .any(|t| TOKEN_TYPES[t[3] as usize] == "property")
        );
    }
    #[test]
    fn positions_handle_utf16_multiline_and_crlf() {
        let source = "// 😀\r\n\"a😀\nb\";";
        let tokens = Tokenizer::new(source).collect::<Vec<_>>();
        let data = encode_tokens(source, &tokens, &HashMap::new());
        assert_eq!(&data[..5], &[0, 0, 5, 8, 0]);
        assert_eq!(data.as_chunks::<5>().0.len(), 3);
        let index = SourceIndex::new("😀 bad\r\n");
        assert_eq!(
            index.position(Span::new(1, 3)),
            Position {
                line: 0,
                character: 3
            }
        );
    }
    #[test]
    fn syntax_errors_keep_lexical_tokens() {
        let document = analyze("fn f() { x := ; }");
        assert!(!document.diagnostics.is_empty());
        assert!(!document.semantic_tokens.is_empty());
        assert_eq!(document.diagnostics[0].diagnostic.kind, ErrorKind::Parse);
    }
}

#[cfg(test)]
mod project_tests {
    fn standalone_plan(path: &std::path::Path) -> crate::plan::BuildPlan {
        super::standalone_plan(path)
            .with_standard_library(crate::standard_library::StandardLibrary::disabled())
    }

    use super::super::source::OverlaySources;
    use super::*;
    #[test]
    fn syntax_failure_blocks_dependencies_but_not_independent_files() {
        let root = PathBuf::from("/editor");
        let mut plan = standalone_plan(&root.join("broken.fib"));
        for (index, name) in ["dependent", "independent"].iter().enumerate() {
            plan.files.push(SourceFile {
                id: FileId((index + 1) as u32),
                name: FileName((*name).into()),
                path: root.join(format!("{name}.fib")),
            });
        }
        let sources = OverlaySources(
            [
                (
                    root.join("broken.fib"),
                    "pub fn value() int32 { return ;".into(),
                ),
                (
                    root.join("dependent.fib"),
                    "fn f() int32 { return broken::value(); }".into(),
                ),
                (
                    root.join("independent.fib"),
                    "fn f() { a: bool = 1; }".into(),
                ),
            ]
            .into_iter()
            .collect(),
        );
        let result = analyze_plan(&plan, &sources, false);
        assert_eq!(
            result.documents[&root.join("broken.fib")].diagnostics.len(),
            1
        );
        assert!(
            result.documents[&root.join("dependent.fib")]
                .diagnostics
                .is_empty()
        );
        assert_eq!(
            result.documents[&root.join("independent.fib")]
                .diagnostics
                .len(),
            1
        );
    }
    #[test]
    fn missing_source_is_an_io_diagnostic_without_name_cascades() {
        let root = tempfile::tempdir().unwrap();
        let mut plan = standalone_plan(&root.path().join("absent.fib"));
        let dependent = root.path().join("dependent.fib");
        plan.files.push(SourceFile {
            id: FileId(1),
            name: FileName("dependent".into()),
            path: dependent.clone(),
        });
        let sources = OverlaySources(
            [(dependent.clone(), "fn f() { absent::value(); }".into())]
                .into_iter()
                .collect(),
        );
        let output = analyze_plan(&plan, &sources, false);
        assert_eq!(
            output.documents[&plan.files[0].path].diagnostics[0]
                .diagnostic
                .kind,
            ErrorKind::Io
        );
        assert!(output.documents[&dependent].diagnostics.is_empty());
    }
}
