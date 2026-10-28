//! Nominal runtime type registration and pointer interning.
use super::*;

pub(super) fn register_runtime_types(
    project: &AnalyzedProject,
    ir: &mut Ir,
) -> anyhow::Result<HashMap<SemanticId, TypeId>> {
    fn supported(
        project: &AnalyzedProject,
        ty: SemanticId,
        seen: &mut std::collections::HashSet<SemanticId>,
    ) -> anyhow::Result<bool> {
        let ty = project.types.canonical(ty)?;
        if !seen.insert(ty) {
            return Ok(matches!(
                project.types.get(ty)?,
                SemanticType::Struct(_) | SemanticType::Enum(_)
            ));
        }
        let result = match project.types.get(ty)? {
            SemanticType::Builtin(b) => *b == BuiltinType::Void || SCALARS[..16].contains(b),
            SemanticType::Readonly(t) => supported(project, *t, seen)?,
            SemanticType::Pointer(t) => {
                matches!(
                    project.types.get(*t)?,
                    SemanticType::Builtin(BuiltinType::Void)
                ) || supported(project, *t, seen)?
            }
            SemanticType::Slice(element) => supported(project, *element, seen)?,
            SemanticType::Function(f) => {
                let mut result = true;
                for ty in &f.parameters {
                    result &= supported(project, *ty, seen)?;
                }
                result
                    && (matches!(
                        project.types.get(f.return_type)?,
                        SemanticType::Builtin(BuiltinType::Void)
                    ) || supported(project, f.return_type, seen)?)
            }
            SemanticType::Array { element, .. } => supported(project, *element, seen)?,
            SemanticType::Struct(fields) => {
                let mut result = true;
                for f in fields {
                    result &= supported(project, f.ty, seen)?;
                }
                result
            }
            SemanticType::Enum(variants) => {
                let mut result = true;
                for v in variants {
                    if let Some(payload) = project.types.enum_payload(v.payload)? {
                        result &= supported(project, payload, seen)?;
                    }
                }
                result
            }
            _ => false,
        };
        seen.remove(&ty);
        Ok(result)
    }
    fn register(
        project: &AnalyzedProject,
        ir: &mut Ir,
        map: &mut HashMap<SemanticId, TypeId>,
        original: SemanticId,
    ) -> anyhow::Result<TypeId> {
        if let Some(&ty) = map.get(&original) {
            return Ok(ty);
        }
        let canonical = project.types.canonical(original)?;
        if let Some(&ty) = map.get(&canonical) {
            map.insert(original, ty);
            return Ok(ty);
        }
        let ty = match project.types.get(canonical)? {
            SemanticType::Builtin(BuiltinType::Void) => {
                if let Some(index) = ir
                    .types
                    .iter()
                    .position(|t| matches!(t, IrType::Tuple(fields) if fields.is_empty()))
                {
                    TypeId(index)
                } else {
                    let id = TypeId(ir.types.len());
                    ir.types.push(IrType::Tuple(vec![]));
                    id
                }
            }
            SemanticType::Builtin(b) => scalar_id(b),
            SemanticType::Readonly(inner) => register(project, ir, map, *inner)?,
            SemanticType::Pointer(inner) => {
                let pointee = if matches!(
                    project.types.get(project.types.canonical(*inner)?)?,
                    SemanticType::Builtin(BuiltinType::Void)
                ) {
                    if let Some(index) = ir.types.iter().position(|t| matches!(t, IrType::Opaque)) {
                        TypeId(index)
                    } else {
                        let id = TypeId(ir.types.len());
                        ir.types.push(IrType::Opaque);
                        id
                    }
                } else {
                    register(project, ir, map, *inner)?
                };
                if let Some(index) = ir
                    .types
                    .iter()
                    .position(|t| *t == IrType::Pointer { pointee })
                {
                    TypeId(index)
                } else {
                    let id = TypeId(ir.types.len());
                    ir.types.push(IrType::Pointer { pointee });
                    id
                }
            }
            SemanticType::Function(f) => {
                let params = f
                    .parameters
                    .iter()
                    .map(|t| register(project, ir, map, *t))
                    .collect::<anyhow::Result<Vec<_>>>()?;
                let results = if matches!(
                    project.types.get(f.return_type)?,
                    SemanticType::Builtin(BuiltinType::Void)
                ) {
                    vec![]
                } else {
                    vec![register(project, ir, map, f.return_type)?]
                };
                let id = TypeId(ir.types.len());
                ir.types.push(IrType::FunctionPointer(Signature {
                    params,
                    results,
                    calling_convention: match f.calling_convention {
                        crate::frontend::semantic_type::CallConvention::C => CallingConvention::C,
                        _ => CallingConvention::Fib,
                    },
                }));
                id
            }
            SemanticType::Slice(element) => {
                let element = register(project, ir, map, *element)?;
                let id = TypeId(ir.types.len());
                ir.types.push(IrType::Slice { element });
                id
            }
            SemanticType::Array { element, length } => {
                let element = register(project, ir, map, *element)?;
                let id = TypeId(ir.types.len());
                ir.types.push(IrType::Array {
                    element,
                    len: *length,
                });
                id
            }
            SemanticType::Struct(fields) => {
                let id = TypeId(ir.types.len());
                map.insert(canonical, id);
                ir.types.push(IrType::Struct {
                    name: None,
                    fields: vec![],
                });
                let fields = fields
                    .iter()
                    .map(|f| register(project, ir, map, f.ty))
                    .collect::<anyhow::Result<Vec<_>>>()?;
                ir.types[id.0] = IrType::Struct { name: None, fields };
                id
            }
            SemanticType::Enum(variants) => {
                let id = TypeId(ir.types.len());
                map.insert(canonical, id);
                ir.types.push(IrType::Enum {
                    name: None,
                    variants: vec![],
                });
                let variants = variants
                    .iter()
                    .enumerate()
                    .map(|(index, v)| {
                        Ok(EnumVariant {
                            name: Some(v.spelling.clone()),
                            discriminant: index as u128,
                            fields: project
                                .types
                                .enum_payload(v.payload)?
                                .map(|p| register(project, ir, map, p))
                                .transpose()?
                                .into_iter()
                                .collect(),
                        })
                    })
                    .collect::<anyhow::Result<Vec<_>>>()?;
                ir.types[id.0] = IrType::Enum {
                    name: None,
                    variants,
                };
                id
            }
            _ => anyhow::bail!("unsupported runtime type registration"),
        };
        map.insert(original, ty);
        map.insert(canonical, ty);
        Ok(ty)
    }
    let mut map = HashMap::new();
    // Reserve all supported nominal identities before resolving recursive pointers.
    let mut nominal = Vec::new();
    for (index, ty) in project.types.types.iter().enumerate() {
        let id = SemanticId(index as u32);
        if matches!(ty, SemanticType::Struct(_) | SemanticType::Enum(_))
            && supported(project, id, &mut std::collections::HashSet::new())?
        {
            let runtime = TypeId(ir.types.len());
            map.insert(id, runtime);
            ir.types.push(IrType::Struct {
                name: None,
                fields: vec![],
            });
            nominal.push((id, runtime));
        }
    }
    for (id, runtime) in nominal {
        ir.types[runtime.0] = match project.types.get(id)? {
            SemanticType::Struct(fields) => IrType::Struct {
                name: None,
                fields: fields
                    .iter()
                    .map(|f| register(project, ir, &mut map, f.ty))
                    .collect::<anyhow::Result<_>>()?,
            },
            SemanticType::Enum(variants) => IrType::Enum {
                name: None,
                variants: variants
                    .iter()
                    .enumerate()
                    .map(|(index, v)| {
                        Ok(EnumVariant {
                            name: Some(v.spelling.clone()),
                            discriminant: index as u128,
                            fields: project
                                .types
                                .enum_payload(v.payload)?
                                .map(|p| register(project, ir, &mut map, p))
                                .transpose()?
                                .into_iter()
                                .collect(),
                        })
                    })
                    .collect::<anyhow::Result<_>>()?,
            },
            _ => unreachable!(),
        };
    }
    for (index, ty) in project.types.types.iter().enumerate() {
        let id = SemanticId(index as u32);
        if matches!(ty, SemanticType::Builtin(b) if SCALARS[..16].contains(b) || (*b == BuiltinType::String && ir.types.get(scalar_id(b).0) == Some(&IrType::Bytes)))
            || supported(project, id, &mut std::collections::HashSet::new())?
        {
            register(project, ir, &mut map, id)?;
        }
    }
    Ok(map)
}
