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
    tokens::{Literal, Punctuation, Token, TokenKind, builtin::BuiltinType},
};
use crate::{
    CompilationUnit,
    diagnostics::{CompilerError, ErrorKind, Span},
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
    pub completions: BTreeMap<String, Vec<Completion>>,
    pub completion_context: Option<std::sync::Arc<CompletionContext>>,
}

/// Immutable project inputs from the same analysis generation as the document.
#[derive(Debug)]
pub struct CompletionContext {
    inputs: std::sync::Arc<CompletionInputs>,
    path: PathBuf,
}

#[derive(Debug)]
struct CompletionInputs {
    plan: BuildPlan,
    sources: HashMap<PathBuf, String>,
    build_file: bool,
}

struct SnapshotSources<'a>(&'a HashMap<PathBuf, String>);
impl SourceProvider for SnapshotSources<'_> {
    fn read(&self, path: &Path) -> std::io::Result<String> {
        self.0.get(path).cloned().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "source unavailable in analysis snapshot",
            )
        })
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct Completion {
    pub label: String,
    pub kind: u32,
    pub detail: String,
}

/// Semantic completion tolerates unfinished expressions and UTF-16 positions.
pub fn complete(document: &DocumentAnalysis, position: Position) -> Vec<Completion> {
    let Some(line) = document.source.split('\n').nth(position.line as usize) else {
        return vec![];
    };
    let mut units = 0;
    let mut offset = 0;
    for c in line.chars() {
        if units == position.character {
            break;
        }
        units += c.len_utf16() as u32;
        offset += c.len_utf8();
    }
    if units != position.character {
        return vec![];
    }
    let index = SourceIndex::new(&document.source);
    if document.tokens.iter().any(|token| {
        let range = index.token_range(token);
        matches!(
            token.kind,
            TokenKind::Comment | TokenKind::Literal(_) | TokenKind::Error(_)
        ) && (range.start.line, range.start.character) <= (position.line, position.character)
            && ((position.line, position.character) < (range.end.line, range.end.character)
                || (matches!(token.kind, TokenKind::Comment | TokenKind::Error(_))
                    && position == range.end))
    }) {
        return vec![];
    }
    let prefix = &line[..offset];
    let start = prefix
        .rfind(|c: char| !(c.is_alphanumeric() || c == '_' || c == ':'))
        .map_or(0, |i| i + prefix[i..].chars().next().unwrap().len_utf8());
    if let Some((namespace, partial)) = prefix[start..].rsplit_once("::") {
        return document
            .completions
            .get(namespace)
            .into_iter()
            .flatten()
            .filter(|item| item.label.starts_with(partial))
            .cloned()
            .collect();
    }
    let field = prefix[..start].trim_end().ends_with('.');
    let key = completion_key(
        field,
        position.line,
        line[..start].encode_utf16().count() as u32,
    );
    let partial = &prefix[start..];
    if let Some(items) = document.completions.get(&key) {
        return items
            .iter()
            .filter(|item| item.label.starts_with(partial))
            .cloned()
            .collect();
    }
    let Some(context) = &document.completion_context else {
        return vec![];
    };
    // Repair only the requested expression. Diagnostics still use the original
    // source; this private query AST can never enter compilation or lowering.
    let line_offset = document
        .source
        .split_inclusive('\n')
        .take(position.line as usize)
        .map(str::len)
        .sum::<usize>();
    let cursor = line_offset + offset;
    let end = document.source[cursor..]
        .char_indices()
        .find(|(_, c)| !(c.is_alphanumeric() || *c == '_'))
        .map_or(document.source.len(), |(i, _)| cursor + i);
    let mut source = document.source.clone();
    source.replace_range(line_offset + start..end, "__fib_completion_probe");
    // A common editing state is a missing terminator and unclosed function/body.
    // Try the intact suffix first, then a terminated prefix with balanced delimiters.
    let probe_end = line_offset + start + "__fib_completion_probe".len();
    let mut candidates = vec![source.clone()];
    if source[probe_end..].trim_start().starts_with('}') {
        let mut terminated = source.clone();
        terminated.insert(probe_end, ';');
        candidates.push(terminated);
    }
    source.truncate(probe_end);
    let mut delimiters = Vec::new();
    for token in Tokenizer::new(&source) {
        match token.kind {
            TokenKind::Punctuation(Punctuation::OpeningCurlyBrace) => delimiters.push('}'),
            TokenKind::Punctuation(Punctuation::OpeningParenthesis) => delimiters.push(')'),
            TokenKind::Punctuation(Punctuation::OpeningSquareBrace) => delimiters.push(']'),
            TokenKind::Punctuation(
                Punctuation::ClosingCurlyBrace
                | Punctuation::ClosingParenthesis
                | Punctuation::ClosingSquareBrace,
            ) => {
                delimiters.pop();
            }
            _ => {}
        }
    }
    while delimiters.last().is_some_and(|c| *c != '}') {
        source.push(delimiters.pop().unwrap());
    }
    source.push(';');
    while let Some(delimiter) = delimiters.pop() {
        source.push(delimiter);
        if delimiter == '}' && !delimiters.is_empty() {
            source.push(';');
        }
    }
    candidates.push(source);
    for source in candidates {
        let mut sources = context.inputs.sources.clone();
        sources.insert(context.path.clone(), source);
        let result = analyze_plan_inner(
            &context.inputs.plan,
            &SnapshotSources(&sources),
            context.inputs.build_file,
            false,
        );
        if let Some(items) = result
            .documents
            .get(&context.path)
            .and_then(|d| d.completions.get(&key))
        {
            return items
                .iter()
                .filter(|item| item.label.starts_with(partial))
                .cloned()
                .collect();
        }
    }
    vec![]
}

fn completion_key(field: bool, line: u32, character: u32) -> String {
    format!(
        "{}:{line}:{character}",
        if field { "field" } else { "local" }
    )
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
                completions: BTreeMap::new(),
                completion_context: None,
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
    analyze_plan_inner(plan, sources, build_file, true)
}

fn analyze_plan_inner(
    plan: &BuildPlan,
    sources: &dyn SourceProvider,
    build_file: bool,
    retain_context: bool,
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
    let mut snapshot_sources = HashMap::new();
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
        if retain_context {
            snapshot_sources.insert(file.path.clone(), source.clone());
        }
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
                completions: BTreeMap::new(),
                completion_context: None,
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
    if retain_context {
        let inputs = std::sync::Arc::new(CompletionInputs {
            plan: plan.clone(),
            sources: snapshot_sources,
            build_file,
        });
        for (path, document) in &mut output.documents {
            document.completion_context = Some(std::sync::Arc::new(CompletionContext {
                inputs: inputs.clone(),
                path: path.clone(),
            }));
        }
    }
    output
}

fn apply_report(
    output: &mut ProjectAnalysis,
    plan: &BuildPlan,
    mut report: AnalysisReport,
    mut roles: HashMap<FileId, HashMap<Span, &'static str>>,
) {
    for file in &plan.files {
        let Some(document) = output.documents.get_mut(&file.path) else {
            continue;
        };
        let symbols = &report.symbols;
        let index = SourceIndex::new(&document.source);
        for ((owner, span), bindings) in &report.lexical_completions {
            if *owner != file.id {
                continue;
            }
            let position = index.position(*span);
            let items = bindings
                .keys()
                .map(|name| Completion {
                    label: name.clone(),
                    kind: 6,
                    detail: "lexical binding".into(),
                })
                .collect();
            document.completions.insert(
                completion_key(false, position.line, position.character),
                items,
            );
        }
        let mut paths: BTreeMap<String, FileId> = symbols
            .files_by_name
            .iter()
            .filter(|(_, id)| {
                symbols.regions.get(id).is_none_or(|r| {
                    symbols
                        .regions
                        .get(&file.id)
                        .is_some_and(|f| f.package == r.package)
                })
            })
            .map(|(name, id)| (name.0.clone(), *id))
            .collect();
        if let Some(region) = symbols.regions.get(&file.id) {
            for ((package, path), id) in &symbols.package_files {
                if *package == region.package {
                    paths.insert(format!("root::{path}"), *id);
                }
                if let Some(dependencies) = symbols.dependencies.get(&region.package) {
                    for (name, target) in dependencies {
                        if target == package {
                            paths.insert(format!("{name}::{path}"), *id);
                        }
                    }
                }
            }
        }
        for (path, owner) in paths {
            let segments: Vec<_> = path.split("::").collect();
            for n in 1..segments.len() {
                document
                    .completions
                    .entry(segments[..n].join("::"))
                    .or_default()
                    .push(Completion {
                        label: segments[n].into(),
                        kind: 9,
                        detail: "module".into(),
                    });
            }
            if let Some(namespace) = symbols.namespaces.get(&owner) {
                for (name, id) in &namespace.names {
                    let declaration = &namespace.declarations[id.index as usize];
                    if !symbols.accessible(file.id, owner, declaration.visibility) {
                        continue;
                    }
                    let info = report
                        .types
                        .bindings
                        .get(&super::analysis_annotation::BindingId::Declaration(*id));
                    let kind = if info.is_some_and(|info| {
                        matches!(
                            report
                                .types
                                .canonical(info.ty)
                                .and_then(|ty| report.types.get(ty)),
                            Ok(SemanticType::Function(_) | SemanticType::IntrinsicFunction)
                        )
                    }) {
                        3
                    } else if info.is_some_and(|info| info.type_value.is_some()) {
                        7
                    } else {
                        6
                    };
                    document
                        .completions
                        .entry(path.clone())
                        .or_default()
                        .push(Completion {
                            label: name.clone(),
                            kind,
                            detail: format!("{path}::{name}"),
                        });
                }
            }
        }
        for items in document.completions.values_mut() {
            items.sort_by(|a, b| a.label.cmp(&b.label));
            items.dedup_by(|a, b| a.label == b.label);
        }
    }
    for unit in &mut report.units {
        let Some(file) = plan.files.iter().find(|f| f.id == unit.source_file_id) else {
            continue;
        };
        let Some(document) = output.documents.get_mut(&file.path) else {
            continue;
        };
        let mut classifier = Classifier {
            completions: &mut document.completions,
            index: SourceIndex::new(&document.source),
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
    completions: &'a mut BTreeMap<String, Vec<Completion>>,
    index: SourceIndex<'a>,
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
        if let ExpressionKind::ExprAcess { expr, member, .. } = &node.kind
            && let (Some(span), Some(info)) = (member.source_span, expr.annotation.resolved)
            && info.type_value.is_none()
        {
            let mut ty = info.ty;
            if let Ok((SemanticType::Pointer(inner), _)) = self.types.container(ty) {
                ty = *inner;
            }
            if let Ok((SemanticType::Struct(fields), _)) = self.types.container(ty) {
                let position = self.index.position(span.into());
                self.completions.insert(
                    completion_key(true, position.line, position.character),
                    fields
                        .iter()
                        .map(|field| Completion {
                            label: field.spelling.clone(),
                            kind: 5,
                            detail: "struct field".into(),
                        })
                        .collect(),
                );
            }
        }
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

    fn suggestions(marked: &str) -> Vec<Completion> {
        let cursor = marked.find('|').unwrap();
        let source = marked.replacen('|', "", 1);
        let prefix = &marked[..cursor];
        let position = Position {
            line: prefix.bytes().filter(|b| *b == b'\n').count() as u32,
            character: prefix.rsplit('\n').next().unwrap().encode_utf16().count() as u32,
        };
        complete(&analyze(&source), position)
    }

    #[test]
    fn completion_uses_lexical_scope_and_declaration_order() {
        let items =
            suggestions("fn f(parameter: int32) { local_value := 42; loc|al_value; later := 1; }");
        assert_eq!(
            items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(),
            ["local_value"]
        );
        assert_eq!(
            suggestions("fn f(parameter: int32) { par|ameter; }")[0].label,
            "parameter"
        );
        let items = suggestions(
            "fn other(hidden: int32) {} fn f(parameter: int32) { if (true) { inner := 1; }; outer := 2; | later := 3; }",
        );
        let labels = items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>();
        assert_eq!(labels, ["outer", "parameter"]);
        assert!(suggestions("fn f() { later|; later := 3; }").is_empty());
        assert!(suggestions("fn f(outer: int32) { fn inner() { out|; } }").is_empty());
    }

    #[test]
    fn completion_handles_incomplete_input_and_utf16() {
        for source in [
            "fn f(parameter: int32) { local_value := 42; local_v|",
            "fn f(parameter: int32) { local_value := 42; local_v| }",
            "fn f(parameter: int32) { local_value := 42; s := \"😀\"; local_v|; }",
            "fn f(parameter: int32) {\r\n local_value := 42;\r\n local_v|",
            "fn consume(x: int32) {} fn f() { local_value := 42; consume(local_v|",
            "fn f() { local_value := 42; if (true) { local_v|",
        ] {
            assert_eq!(suggestions(source)[0].label, "local_value", "{source}");
        }
        assert_eq!(
            suggestions("fn f(parameter: int32) { | ")[0].label,
            "parameter"
        );
    }

    #[test]
    fn completion_uses_receiver_type_including_shadowing_and_pointers() {
        let declarations =
            "comp A := struct { alpha: int32, another: int32 }; comp B := struct { beta: int32 }; ";
        for suffix in [
            "fn f(value: A) { value.a|lpha; }",
            "fn f(value: A) { value.a|",
            "fn f(value: A) { value.| }",
            "fn f(value: *A) { value.|",
            "fn f(value: A) { s := \"😀\"; value.a|; }",
            "fn f(value: A) { if (true) { value.a|",
        ] {
            let items = suggestions(&format!("{declarations}{suffix}"));
            assert_eq!(
                items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(),
                ["alpha", "another"],
                "{suffix}"
            );
            assert!(items.iter().all(|i| i.kind == 5));
        }
        let items = suggestions(&format!(
            "{declarations}value: A = A {{ alpha: 1, another: 2 }}; fn f(value: B) {{ value.b|; }}"
        ));
        assert_eq!(items[0].label, "beta");
        assert!(suggestions(&format!("{declarations}fn f(value: A) {{ value.z|; }}")).is_empty());
        assert!(suggestions(&format!("{declarations}fn f() {{ A.a|; }}")).is_empty());
    }

    #[test]
    fn completion_handles_chained_receivers_and_original_issue_examples() {
        assert_eq!(
            suggestions("fn f() { local_value := 42; local_v|alue; }")[0].label,
            "local_value"
        );
        assert_eq!(suggestions("comp S := struct { member: int32 }; fn f() { value := S { member: 42 }; value.mem|ber; }")[0].label, "member");
        assert_eq!(suggestions("comp S := struct { member: int32 }; comp Outer := struct { child: S }; fn f(value: Outer) { value.child.mem|; }")[0].label, "member");
        assert_eq!(
            suggestions(
                "comp S := struct { member: int32 }; fn f(values: []S) { values[0].mem|; }"
            )[0]
            .label,
            "member"
        );
        assert_eq!(suggestions("comp S := struct { member: int32 }; fn make() S { return S { member: 1 }; } fn f() { make().mem|; }")[0].label, "member");
    }

    #[test]
    fn completion_is_suppressed_in_comments_and_literals() {
        for source in [
            "fn f(local: int32) { \"loc|al\"; }",
            "fn f(local: int32) { // loc|al\n }",
            "fn f(local: int32) { /* loc|al */ }",
            "fn f(local: int32) { // loc|",
            "fn f(local: int32) { \"loc|",
        ] {
            assert!(suggestions(source).is_empty(), "{source}");
        }
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
    fn completion_respects_cross_file_visibility_and_frozen_overlays() {
        let root = tempfile::tempdir().unwrap();
        let main = root.path().join("main.fib");
        let helper = root.path().join("helper.fib");
        let mut plan = standalone_plan(&main);
        plan.files.push(SourceFile {
            id: FileId(1),
            name: FileName("helper".into()),
            path: helper.clone(),
        });
        let source = "fn f(value: helper::Public) { value.mem";
        let sources = OverlaySources([
            (main.clone(), source.into()),
            (helper.clone(), "pub comp Public := struct { member: int32 }; comp Private := struct { hidden: int32 };".into()),
        ].into_iter().collect());
        let analysis = analyze_plan(&plan, &sources, false);
        let document = &analysis.documents[&main];
        // A later disk edit must not leak into the retained analysis generation.
        std::fs::write(&helper, "pub comp Public := struct { replacement: int32 };").unwrap();
        let items = complete(
            document,
            Position {
                line: 0,
                character: source.len() as u32,
            },
        );
        assert_eq!(
            items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(),
            ["member"]
        );
        let sources = OverlaySources(
            [
                (main.clone(), "fn f() { helper::".into()),
                (helper, sources.0[&root.path().join("helper.fib")].clone()),
            ]
            .into_iter()
            .collect(),
        );
        let analysis = analyze_plan(&plan, &sources, false);
        let items = complete(
            &analysis.documents[&main],
            Position {
                line: 0,
                character: 17,
            },
        );
        assert_eq!(
            items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(),
            ["Public"]
        );
    }

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
