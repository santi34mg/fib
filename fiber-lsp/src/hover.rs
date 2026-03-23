use fibc::driver::FrontendResult;
use fibc::hir::{HIRBinding, HIRFunction, HIRSymbol, HIRTypeKind};
use fibc::token::TokenKind;
use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind};

use crate::lookup::{find_symbol, token_at};

pub fn hover_info(result: &FrontendResult, line: usize, col: usize) -> Option<Hover> {
    let tok = token_at(&result.tokens, line, col)?;
    let name = match &tok.kind {
        TokenKind::Identifier(id) => id,
        _ => return None,
    };

    let hir = result.hir.as_ref()?;
    let symbol = find_symbol(name, &hir.scope_root)?;

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
        HIRSymbol::Binding(b) => format_binding(b),
        HIRSymbol::Type(t) => format!("type {} = {}", name, format_type(t)),
    }
}

fn format_function(f: &HIRFunction) -> String {
    let params: Vec<String> = f
        .params
        .iter()
        .map(|(name, ty)| format!("{} {}", name, format_type(ty)))
        .collect();
    let prefix = if f.is_extern { "extern fn" } else { "fn" };
    format!("{} {}({}) {}", prefix, f.name, params.join(", "), format_type(&f.return_type))
}

fn format_binding(b: &HIRBinding) -> String {
    let kw = if b.mutable { "var" } else { "const" };
    format!("{} {} {}", kw, b.name, format_type(&b.ty))
}

fn format_type(ty: &HIRTypeKind) -> String {
    ty.to_string()
}
