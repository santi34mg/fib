use fibc::driver::FrontendResponse;
use fibc::hir::{GenericFunctionTemplate, HIRBinding, HIRFunction, HIRSymbol, HIRTypeKind};
use fibc::tokens::TokenKind;
use fibc::tokens::punctuation::Punctuation;
use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind};

use crate::lookup::{find_module_symbol, find_symbol, token_at};

pub fn hover_info(result: &FrontendResponse, line: usize, col: usize) -> Option<Hover> {
    let tok = token_at(&result.tokens, line, col)?;
    let name = match &tok.kind {
        TokenKind::Identifier(id) => id,
        _ => return None,
    };

    let hir = result.hir.as_ref()?;

    // Check if this identifier is preceded by `module ::` (qualified access).
    // If so, look it up in the module's exports instead of the root scope.
    let tok_idx = result.tokens.iter().position(|t| std::ptr::eq(t, tok))?;
    let symbol = if tok_idx >= 2 {
        let maybe_dcolon = &result.tokens[tok_idx - 1];
        let maybe_module = &result.tokens[tok_idx - 2];
        if matches!(
            maybe_dcolon.kind,
            TokenKind::Punctuation(Punctuation::DoubleColon)
        ) {
            if let TokenKind::Identifier(module_id) = &maybe_module.kind {
                find_module_symbol(module_id.identifier.as_str(), name, &hir.scope_root)
            } else {
                find_symbol(name, &hir.scope_root)
            }
        } else {
            find_symbol(name, &hir.scope_root)
        }
    } else {
        find_symbol(name, &hir.scope_root)
    }?;

    let text = format_symbol(name.identifier.as_str(), symbol);
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: format!("```fiber\n{}\n```", text),
        }),
        range: None,
    })
}

fn format_symbol(name: &str, symbol: &HIRSymbol) -> String {
    match symbol {
        HIRSymbol::Function(f) => format_function(f),
        HIRSymbol::GenericFunction(g) => format_generic_function(g),
        HIRSymbol::Binding(b) => format_binding(b),
        HIRSymbol::Type(t) => format!("type {} = {}", name, format_type(t)),
    }
}

fn format_generic_function(g: &GenericFunctionTemplate) -> String {
    let params: Vec<String> = g
        .ast_decl
        .signature
        .parameters
        .iter()
        .map(|p| format!("{} {}", p.parameter_name, p.parameter_type))
        .collect();
    let ret = g
        .ast_decl
        .signature
        .return_type
        .as_ref()
        .map(|t| format!(" {}", t))
        .unwrap_or_default();
    let prefix = if g.ast_decl.is_extern {
        "extern fn"
    } else {
        "fn"
    };
    format!(
        "{} {}({}){}",
        prefix,
        g.ast_decl.signature.name,
        params.join(", "),
        ret
    )
}

fn format_function(f: &HIRFunction) -> String {
    let params: Vec<String> = f
        .params
        .iter()
        .map(|(name, ty)| format!("{} {}", name, format_type(ty)))
        .collect();
    let prefix = if f.is_extern { "extern fn" } else { "fn" };
    format!(
        "{} {}({}) {}",
        prefix,
        f.name,
        params.join(", "),
        format_type(&f.return_type)
    )
}

fn format_binding(b: &HIRBinding) -> String {
    let kw = if b.mutable { "var" } else { "const" };
    format!("{} {} {}", kw, b.name, format_type(&b.ty))
}

fn format_type(ty: &HIRTypeKind) -> String {
    ty.to_string()
}
