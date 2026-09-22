//! Unit tests for `SymbolTable` scope discipline and `Ty` display.
//! Locks in push/pop/leak behavior, shadowing, and module registration.

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::frontend::identifier::Identifier;
    use crate::frontend::tokens::builtin::BuiltinType;

    fn id(name: &str) -> Identifier {
        Identifier {
            value: name.to_string(),
        }
    }

    fn binding(name: &str) -> TypedSymbol {
        TypedSymbol::Binding(TypedBinding {
            name: id(name),
            ty: Ty::Builtin(BuiltinType::Int4),
            init: None,
            mutable: true,
        })
    }

    #[test]
    fn insert_and_lookup_in_global_scope() {
        let mut st = SymbolTable::new();
        assert_eq!(st.depth(), 1);
        assert!(st.lookup(&id("x")).is_none());
        st.insert(id("x"), binding("x"));
        assert!(st.lookup(&id("x")).is_some());
    }

    #[test]
    fn inner_scope_shadows_outer_without_clobbering() {
        let mut st = SymbolTable::new();
        st.insert(id("x"), binding("x"));
        st.enter_scope(ScopeKind::Block);
        st.insert(
            id("x"),
            TypedSymbol::Binding(TypedBinding {
                name: id("x"),
                ty: Ty::Builtin(BuiltinType::Boolean),
                init: None,
                mutable: false,
            }),
        );
        let found = st.lookup(&id("x")).expect("shadowed binding");
        assert!(matches!(
            found,
            TypedSymbol::Binding(b) if b.ty == Ty::Builtin(BuiltinType::Boolean)
        ));
        st.exit_scope();
        // Outer binding is intact after the inner scope is dropped.
        let found = st.lookup(&id("x")).expect("outer binding");
        assert!(matches!(
            found,
            TypedSymbol::Binding(b) if b.ty == Ty::Builtin(BuiltinType::Int4)
        ));
    }

    #[test]
    fn exit_scope_discards_locals_so_they_never_leak() {
        let mut st = SymbolTable::new();
        st.enter_scope(ScopeKind::Function);
        st.insert(id("tmp"), binding("tmp"));
        assert_eq!(st.depth(), 2);
        st.exit_scope();
        assert_eq!(st.depth(), 1);
        assert!(st.lookup(&id("tmp")).is_none());
    }

    // Note: unbalanced `exit_scope` on the global frame is a `debug_assert`
    // in debug builds (intentional fail-fast for compiler bugs) and a no-op
    // in release — so there is deliberately no unit test pinning either.

    #[test]
    fn remove_takes_from_innermost_scope_first() {
        let mut st = SymbolTable::new();
        st.insert(id("x"), binding("x"));
        st.enter_scope(ScopeKind::Block);
        st.insert(id("x"), binding("x"));
        assert!(st.remove(&id("x")).is_some());
        // Inner copy gone, outer still visible.
        assert!(st.lookup(&id("x")).is_some());
        assert!(st.remove(&id("x")).is_some());
        assert!(st.lookup(&id("x")).is_none());
    }

    #[test]
    fn global_symbols_only_sees_frame_zero() {
        let mut st = SymbolTable::new();
        st.insert(id("g"), binding("g"));
        st.enter_scope(ScopeKind::Block);
        st.insert(id("local"), binding("local"));
        let globals = st.global_symbols();
        assert!(globals.contains_key(&id("g")));
        assert!(!globals.contains_key(&id("local")));
    }

    #[test]
    fn modules_register_lookup_and_iterate() {
        let mut st = SymbolTable::new();
        assert!(st.lookup_module("m").is_none());
        let mut exports = HashMap::new();
        exports.insert(id("foo"), binding("foo"));
        st.insert_module(
            "m".to_string(),
            TypedModule {
                name: "m".to_string(),
                path: vec![id("m")],
                exports,
                declarations: Vec::new(),
            },
        );
        let module = st.lookup_module("m").expect("module");
        assert!(module.exports.contains_key(&id("foo")));
        assert_eq!(st.modules().count(), 1);
    }

    #[test]
    fn ty_display_renders_common_shapes() {
        assert_eq!(Ty::Builtin(BuiltinType::Int4).to_string(), "@int4");
        assert_eq!(
            Ty::Pointer(Box::new(Ty::Builtin(BuiltinType::Int4))).to_string(),
            "*@int4"
        );
        assert_eq!(
            Ty::Array {
                element_type: Box::new(Ty::Builtin(BuiltinType::Int4)),
                size: 8,
            }
            .to_string(),
            "@int4[8]"
        );
        assert_eq!(
            Ty::Slice(Box::new(Ty::Builtin(BuiltinType::Int4))).to_string(),
            "@int4[]"
        );
        assert_eq!(
            Ty::QualifiedIdentifier {
                module: "m".to_string(),
                name: id("Point"),
            }
            .to_string(),
            "m::Point"
        );
    }

    #[test]
    fn block_terminates_spots_terminators() {
        assert!(!TypedIf::block_terminates(&[]));
        assert!(TypedIf::block_terminates(&[TypedStatement::Break]));
        assert!(TypedIf::block_terminates(&[TypedStatement::Return(None)]));
        let tif = TypedIf {
            cond: TypedExpr {
                inferred_type: Ty::Builtin(BuiltinType::Boolean),
                expression: TypedExprKind::LiteralBool(true),
            },
            then_branch: vec![TypedStatement::Continue],
            else_branch: None,
        };
        assert!(tif.then_branch_terminates());
        assert!(!tif.else_branch_terminates());
    }
}
