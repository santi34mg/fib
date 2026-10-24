use super::*;
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
fn unsupported_conventions_and_types_never_fall_back_to_c() {
    for mutate in [
        |s: &mut Signature| s.params[0] = TypeId(1),
        |s: &mut Signature| s.results.push(TypeId(0)),
        |s: &mut Signature| s.params[0] = TypeId(99),
    ] {
        let mut signature = signature();
        mutate(&mut signature);
        assert!(classify_signature(&signature, &types(), &target()).is_err());
    }
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
