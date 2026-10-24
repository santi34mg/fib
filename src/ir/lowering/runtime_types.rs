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
            return Ok(matches!(project.types.get(ty)?, SemanticType::Struct(_)));
        }
        let result = match project.types.get(ty)? {
            SemanticType::Builtin(b) => SCALARS[..15].contains(b),
            SemanticType::Readonly(t) | SemanticType::Pointer(t) => supported(project, *t, seen)?,
            SemanticType::Struct(fields) => {
                let mut result = true;
                for f in fields {
                    result &= supported(project, f.ty, seen)?;
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
            SemanticType::Builtin(b) => scalar_id(b),
            SemanticType::Readonly(inner) => register(project, ir, map, *inner)?,
            SemanticType::Pointer(inner) => {
                let pointee = register(project, ir, map, *inner)?;
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
            _ => anyhow::bail!("unsupported runtime type registration"),
        };
        map.insert(original, ty);
        map.insert(canonical, ty);
        Ok(ty)
    }
    let mut map = HashMap::new();
    let mut structs = Vec::new();
    // Reserve every nominal struct before any pointer registration. This makes
    // recursive fields finite and interns pointers after alias/readonly erasure.
    for (index, ty) in project.types.types.iter().enumerate() {
        let id = SemanticId(index as u32);
        if let SemanticType::Struct(fields) = ty
            && supported(project, id, &mut std::collections::HashSet::new())?
        {
            let runtime = TypeId(ir.types.len());
            map.insert(id, runtime);
            ir.types.push(IrType::Struct {
                name: None,
                fields: vec![],
            });
            structs.push((runtime, fields));
        }
    }
    for (runtime, fields) in structs {
        let fields = fields
            .iter()
            .map(|f| register(project, ir, &mut map, f.ty))
            .collect::<anyhow::Result<Vec<_>>>()?;
        ir.types[runtime.0] = IrType::Struct { name: None, fields };
    }
    for (index, ty) in project.types.types.iter().enumerate() {
        let id = SemanticId(index as u32);
        if matches!(ty, SemanticType::Builtin(b) if SCALARS.contains(b))
            || supported(project, id, &mut std::collections::HashSet::new())?
        {
            register(project, ir, &mut map, id)?;
        }
    }
    Ok(map)
}
