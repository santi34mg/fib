use super::*;
use crate::backend::lowering::c_abi::{Boundary, CSignature, Piece, PieceKind};
use crate::ir::{CallingConvention, IntegerWidth};

fn target() -> AbiTarget {
    AbiTarget {
        triple: "x86_64-unknown-linux-gnu".into(),
        data_layout: "e-p:64:64".into(),
    }
}
fn types() -> Vec<IrType> {
    vec![
        IrType::Integer {
            width: IntegerWidth::Bits(32),
            signed: true,
        },
        IrType::Bool,
    ]
}
fn signature() -> Signature {
    Signature {
        params: vec![TypeId(0), TypeId(0)],
        results: vec![TypeId(0)],
        calling_convention: CallingConvention::C,
    }
}
#[test]
fn c_scalar_classification_is_logical_to_physical_and_target_aware() {
    let signature = signature();
    let abi = classify_signature(&signature, &types(), &target()).unwrap();
    assert_eq!(abi.parameters, vec![TypeId(0), TypeId(0)]);
    assert_eq!(abi.parameter_positions, vec![0, 1]);
    assert_eq!(abi.result, Some(TypeId(0)));
    assert_eq!(abi.convention, PhysicalConvention::C);
    abi.validate(&signature, &types()).unwrap();
}
#[test]
fn unsupported_types_and_results_never_fall_back_to_another_convention() {
    let mut types = types();
    types.push(IrType::Integer {
        width: IntegerWidth::Bits(0),
        signed: true,
    });
    for convention in [CallingConvention::C, CallingConvention::Fib] {
        for mutate in [
            |s: &mut Signature| s.params[0] = TypeId(2),
            |s: &mut Signature| s.results.push(TypeId(0)),
            |s: &mut Signature| s.params[0] = TypeId(99),
            |s: &mut Signature| s.results[0] = TypeId(99),
        ] {
            let mut signature = signature();
            signature.calling_convention = convention;
            mutate(&mut signature);
            assert!(classify_signature(&signature, &types, &target()).is_err());
        }
    }
}

#[test]
fn c_bool_boundaries_are_supported() {
    let signature = Signature {
        params: vec![TypeId(1)],
        results: vec![TypeId(1)],
        calling_convention: CallingConvention::C,
    };
    let abi = classify_signature(&signature, &types(), &target()).unwrap();
    assert_eq!(abi.convention, PhysicalConvention::C);
    assert_eq!(abi.parameters, signature.params);
    assert_eq!(abi.result, Some(TypeId(1)));
    assert_eq!(
        abi.c_abi,
        Some(CSignature {
            arguments: vec![Boundary::Direct],
            result: Some(Boundary::Direct),
        })
    );
}
#[test]
fn void_and_invalid_physical_mappings_are_checked() {
    let mut signature = signature();
    signature.results.clear();
    let mut abi = classify_signature(&signature, &types(), &target()).unwrap();
    assert_eq!(abi.result, None);
    abi.parameter_positions = vec![1, 0];
    abi.validate(&signature, &types()).unwrap();
    abi.parameter_positions = vec![0, 0];
    assert!(abi.validate(&signature, &types()).is_err());
    abi.parameter_positions = vec![0, 99];
    assert!(abi.validate(&signature, &types()).is_err());
}

#[test]
fn convention_and_c_boundary_plan_must_match_the_signature() {
    for convention in [CallingConvention::C, CallingConvention::Fib] {
        let mut signature = signature();
        signature.calling_convention = convention;
        let abi = classify_signature(&signature, &types(), &target()).unwrap();
        let mut invalid = abi.clone();
        invalid.convention = match convention {
            CallingConvention::C => PhysicalConvention::FibDirect,
            CallingConvention::Fib => PhysicalConvention::C,
        };
        assert!(invalid.validate(&signature, &types()).is_err());
        let mut invalid = abi;
        invalid.c_abi = match convention {
            CallingConvention::C => None,
            CallingConvention::Fib => Some(CSignature {
                arguments: vec![Boundary::Direct; 2],
                result: Some(Boundary::Direct),
            }),
        };
        assert!(invalid.validate(&signature, &types()).is_err());
    }
    let signature = signature();
    let abi = classify_signature(&signature, &types(), &target()).unwrap();
    let mut invalid = abi.clone();
    invalid.c_abi.as_mut().unwrap().arguments.pop();
    assert!(invalid.validate(&signature, &types()).is_err());
    let mut invalid = abi;
    invalid.c_abi.as_mut().unwrap().result = None;
    assert!(invalid.validate(&signature, &types()).is_err());
}

#[test]
fn fib_struct_and_pointer_boundaries_remain_distinct_from_c() {
    let mut types = types();
    types.push(IrType::Struct {
        name: None,
        fields: vec![TypeId(0), TypeId(1)],
    });
    types.push(IrType::Pointer { pointee: TypeId(2) });
    let mut signature = Signature {
        params: vec![TypeId(2), TypeId(3)],
        results: vec![TypeId(2)],
        calling_convention: CallingConvention::Fib,
    };
    let mut abi = classify_signature(&signature, &types, &target()).unwrap();
    assert_eq!(abi.convention, PhysicalConvention::FibDirect);
    abi.validate(&signature, &types).unwrap();
    abi.convention = PhysicalConvention::C;
    assert!(abi.validate(&signature, &types).is_err());
    signature.calling_convention = CallingConvention::C;
    let c = classify_signature(&signature, &types, &target()).unwrap();
    assert_eq!(c.convention, PhysicalConvention::C);
    assert_eq!(c.parameters, signature.params);
    assert_eq!(c.result, Some(TypeId(2)));
    let coerced = Boundary::Coerce(vec![Piece {
        offset: 0,
        kind: PieceKind::Integer(40),
    }]);
    assert_eq!(
        c.c_abi,
        Some(CSignature {
            arguments: vec![coerced.clone(), Boundary::Direct],
            result: Some(coerced),
        })
    );
    c.validate(&signature, &types).unwrap();
}

#[test]
fn fib_enum_boundaries_use_the_shared_direct_abi_and_c_rejects_them() {
    let mut types = types();
    types.push(IrType::Enum {
        name: None,
        variants: vec![crate::ir::EnumVariant {
            name: None,
            discriminant: 0,
            fields: vec![TypeId(0)],
        }],
    });
    let mut signature = Signature {
        params: vec![TypeId(2)],
        results: vec![TypeId(2)],
        calling_convention: CallingConvention::Fib,
    };
    let abi = classify_signature(&signature, &types, &target()).unwrap();
    assert_eq!(abi.parameters, vec![TypeId(2)]);
    assert_eq!(abi.result, Some(TypeId(2)));
    signature.calling_convention = CallingConvention::C;
    assert!(classify_signature(&signature, &types, &target()).is_err());
}

fn apple_arm64() -> AbiTarget {
    AbiTarget {
        triple: "arm64-apple-macosx14.0.0".into(),
        data_layout: "e-m:o-i64:64-i128:128-n32:64-S128-Fn32".into(),
    }
}

#[test]
fn aapcs64_classifies_composites_like_clang() {
    use crate::ir::FloatWidth;
    let int32 = TypeId(0);
    let mut types = types();
    let f32 = TypeId(types.len());
    types.push(IrType::Float(FloatWidth::F32));
    let f64 = TypeId(types.len());
    types.push(IrType::Float(FloatWidth::F64));
    let int128 = TypeId(types.len());
    types.push(IrType::Integer {
        width: IntegerWidth::Bits(128),
        signed: true,
    });
    let mut add = |fields: Vec<TypeId>| {
        types.push(IrType::Struct { name: None, fields });
        TypeId(types.len() - 1)
    };
    let pair = add(vec![int32, int32]);
    let triple = add(vec![int32, int32, int32]);
    let vec3 = add(vec![f32, f32, f32]);
    let vec2d = add(vec![f64, f64]);
    let mixed = add(vec![f64, int32]);
    let five_floats = add(vec![f32, f32, f32, f32, f32]);
    let wide = add(vec![int128]);
    let big = add(vec![int128, int32]);
    let coerce = |kind| Boundary::Coerce(vec![Piece { offset: 0, kind }]);
    for (ty, expected) in [
        (pair, coerce(PieceKind::Integer(64))),
        (triple, coerce(PieceKind::Integers64(2))),
        (vec3, coerce(PieceKind::Floats32(3))),
        (vec2d, coerce(PieceKind::Floats64(2))),
        (mixed, coerce(PieceKind::Integers64(2))),
        (
            five_floats,
            Boundary::Indirect {
                alignment: 4,
                byval: false,
            },
        ),
        (wide, coerce(PieceKind::Integer(128))),
        (
            big,
            Boundary::Indirect {
                alignment: 16,
                byval: false,
            },
        ),
        (int32, Boundary::Direct),
    ] {
        let signature = Signature {
            params: vec![ty],
            results: vec![ty],
            calling_convention: CallingConvention::C,
        };
        let abi = classify_signature(&signature, &types, &apple_arm64()).unwrap();
        let c = abi.c_abi.unwrap();
        assert_eq!(c.arguments, vec![expected.clone()], "{:?}", types[ty.0]);
        assert_eq!(c.result, Some(expected));
    }
    // System V keeps copying large arguments into the callee frame (byval).
    let signature = Signature {
        params: vec![big],
        results: vec![],
        calling_convention: CallingConvention::C,
    };
    let sysv = classify_signature(&signature, &types, &target()).unwrap();
    assert_eq!(
        sysv.c_abi.unwrap().arguments,
        vec![Boundary::Indirect {
            alignment: 16,
            byval: true
        }]
    );
}

#[test]
fn c_abi_targets_are_explicit() {
    for (triple, supported) in [
        ("x86_64-unknown-linux-gnu", true),
        ("x86_64-apple-darwin", true),
        ("aarch64-unknown-linux-gnu", true),
        ("arm64-apple-macosx14.0.0", true),
        ("x86_64-unknown-freebsd", false),
        ("riscv64-unknown-linux-gnu", false),
        ("x86_64-pc-windows-msvc", false),
    ] {
        let target = AbiTarget {
            triple: triple.into(),
            data_layout: "e".into(),
        };
        let result = classify_signature(&signature(), &types(), &target);
        assert_eq!(result.is_ok(), supported, "{triple}");
        if let Err(error) = result {
            assert!(
                error.to_string().contains("unsupported for target"),
                "{error}"
            );
        }
    }
}
