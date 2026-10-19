//! Borrowed capability for future privileged emission of exactly verified stores.
//! Neither source spans nor structurally equivalent cloned IR grant this capability.
use super::{MemoSite, error, verify};
use crate::portable::{
    Diagnostic,
    hir::Nominal,
    ir::{Function, Instruction, Operation},
    resolve::Phase,
};
use std::collections::BTreeMap;

struct Target<'ir> {
    source: MemoSite,
    function: &'ir Function,
    instruction: &'ir Instruction,
}

pub(in crate::portable) struct VerifiedMemoPlan<'ir> {
    root: &'ir Function,
    phase: Phase,
    targets: Vec<Target<'ir>>,
}

pub(in crate::portable) struct MemoEncoding {
    pub index: usize,
    pub field: usize,
}

pub(in crate::portable) struct Emission<'plan, 'ir> {
    plan: &'plan VerifiedMemoPlan<'ir>,
    selected: Vec<bool>,
    failed: Option<usize>,
}

pub(in crate::portable::bootstrap) fn verified_plan<'ir>(
    source: &str,
    root: &'ir Function,
    phase: Phase,
) -> Result<VerifiedMemoPlan<'ir>, Diagnostic> {
    let sites = verify(source, root, phase)?;
    let positions: BTreeMap<_, _> = sites
        .iter()
        .enumerate()
        .map(|(index, site)| ((site.span.start, site.span.end), index))
        .collect();
    let mut bound = vec![None; sites.len()];
    let mut pending = vec![root];
    while let Some(function) = pending.pop() {
        for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
            match &instruction.operation {
                Operation::MakeClosure { body, .. } => pending.push(&body.function),
                Operation::MakeGeneralClosure { body, .. } => {
                    pending.extend(body.methods.iter().map(|method| &method.function))
                }
                Operation::MakeFuture { body, .. } => pending.push(&body.continuation.function),
                Operation::Nominal {
                    operation: Nominal::FieldSet(_),
                    ..
                } => {
                    if let Some(index) =
                        positions.get(&(instruction.span.start, instruction.span.end))
                    {
                        if bound[*index].replace((function, instruction)).is_some() {
                            return Err(error(
                                instruction.span.clone(),
                                "Duplicate verified memo instruction",
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let targets = sites
        .into_iter()
        .zip(bound)
        .map(|(source, bound)| {
            let (function, instruction) = bound
                .ok_or_else(|| error(source.span.clone(), "Verified memo instruction was lost"))?;
            Ok(Target {
                source,
                function,
                instruction,
            })
        })
        .collect::<Result<_, Diagnostic>>()?;
    Ok(VerifiedMemoPlan {
        root,
        phase,
        targets,
    })
}

impl<'ir> VerifiedMemoPlan<'ir> {
    pub(in crate::portable) fn root(&self) -> &'ir Function {
        self.root
    }
    pub(in crate::portable) fn phase(&self) -> Phase {
        self.phase
    }
    pub(in crate::portable) fn emission(&self) -> Emission<'_, 'ir> {
        Emission {
            plan: self,
            selected: vec![false; self.targets.len()],
            failed: None,
        }
    }
}

impl Emission<'_, '_> {
    pub(in crate::portable) fn select(
        &mut self,
        function: &Function,
        instruction: &Instruction,
    ) -> Result<Option<MemoEncoding>, Diagnostic> {
        if !matches!(
            instruction.operation,
            Operation::Nominal {
                operation: Nominal::FieldSet(_),
                ..
            }
        ) {
            return Ok(None);
        }
        let Some((index, target)) = self
            .plan
            .targets
            .iter()
            .enumerate()
            .find(|(_, target)| target.source.span == instruction.span)
        else {
            return Ok(None);
        };
        if !std::ptr::eq(target.function, function)
            || !std::ptr::eq(target.instruction, instruction)
        {
            self.failed = Some(index);
            return Err(error(
                instruction.span.clone(),
                "Memo emission requires the exact verified instruction and function",
            ));
        }
        if std::mem::replace(&mut self.selected[index], true) {
            self.failed = Some(index);
            return Err(error(
                instruction.span.clone(),
                "Verified memo instruction emitted more than once",
            ));
        }
        Ok(Some(MemoEncoding {
            index,
            field: target.source.field,
        }))
    }

    pub(in crate::portable) fn finish(self) -> Result<(), Diagnostic> {
        if let Some(index) = self
            .failed
            .or_else(|| self.selected.iter().position(|selected| !selected))
        {
            return Err(error(
                self.plan.targets[index].source.span.clone(),
                "Memo emission did not preserve exact verified coverage",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_memo_emission_has_exact_site_ids_and_ordinary_emission_has_no_capability() {
        let phase = Phase::Macro;
        let analyzed = super::super::super::catalog(phase).unwrap();
        let ir = crate::portable::ir::lower(&analyzed.hir).unwrap();
        let plan = verified_plan(super::super::super::SOURCE, &ir, phase).unwrap();
        let private = crate::portable::emit::emit_memo_plan(&plan).unwrap();
        let ordinary = crate::portable::compile_ir(plan.root()).unwrap();
        for (bytes, expected) in [(&private, 25), (&ordinary, 0)] {
            let mut functions = 0;
            let mut private_index = None;
            let mut seen = std::collections::BTreeSet::new();
            for payload in wasmparser::Parser::new(0).parse_all(bytes) {
                match payload.unwrap() {
                    wasmparser::Payload::ImportSection(imports) => {
                        for import in imports.into_imports() {
                            let import = import.unwrap();
                            if matches!(import.ty, wasmparser::TypeRef::Func(_)) {
                                if import.module
                                    == super::super::super::COMPILER_INTEGRITY_IMPORT_MODULE
                                {
                                    assert_eq!(
                                        import.name,
                                        super::super::super::MEMO_WRITE_IMPORT_NAME
                                    );
                                    assert!(private_index.replace(functions).is_none());
                                }
                                functions += 1;
                            }
                        }
                    }
                    wasmparser::Payload::CodeSectionEntry(body) => {
                        let mut operators = body.get_operators_reader().unwrap();
                        let mut previous = [None; 3];
                        while !operators.eof() {
                            let operator = operators.read().unwrap();
                            if matches!(operator, wasmparser::Operator::Call { function_index } if Some(function_index) == private_index)
                            {
                                let [
                                    Some((false, receiver)),
                                    Some((true, site)),
                                    Some((false, result)),
                                ] = previous
                                else {
                                    panic!("memo argument sequence");
                                };
                                let target = &plan.targets[site as usize];
                                let Operation::Nominal { arguments, .. } =
                                    &target.instruction.operation
                                else {
                                    unreachable!()
                                };
                                assert_eq!(
                                    (receiver, result),
                                    (arguments[0].0 as u32 + 2, arguments[1].0 as u32 + 2)
                                );
                                assert!(seen.insert(site));
                            }
                            previous.rotate_left(1);
                            previous[2] = match operator {
                                wasmparser::Operator::LocalGet { local_index } => {
                                    Some((false, local_index))
                                }
                                wasmparser::Operator::I32Const { value } => {
                                    Some((true, value as u32))
                                }
                                _ => None,
                            };
                        }
                    }
                    _ => {}
                }
            }
            assert_eq!(seen.len(), expected);
            assert_eq!(private_index.is_some(), expected != 0);
            if expected != 0 {
                assert_eq!(seen, (0..25).collect());
            }
        }
    }

    #[test]
    fn canonical_memo_plan_binds_exact_graph_and_rejects_missing_duplicate_and_cloned_stores() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let analyzed = super::super::super::catalog(phase).unwrap();
            let ir = crate::portable::ir::lower(&analyzed.hir).unwrap();
            let plan = verified_plan(super::super::super::SOURCE, &ir, phase).unwrap();
            assert!(std::ptr::eq(plan.root(), &ir));
            assert_eq!(plan.phase(), phase);
            assert_eq!(plan.targets.len(), 25);
            let mut complete = plan.emission();
            for (index, target) in plan.targets.iter().enumerate() {
                let selected = complete
                    .select(target.function, target.instruction)
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    (selected.index, selected.field),
                    (index, target.source.field)
                );
            }
            complete.finish().unwrap();
            assert!(plan.emission().finish().is_err());
            let first = &plan.targets[0];
            let mut duplicate = plan.emission();
            duplicate.select(first.function, first.instruction).unwrap();
            assert!(duplicate.select(first.function, first.instruction).is_err());
            for (index, target) in plan.targets.iter().enumerate() {
                if !duplicate.selected[index] {
                    duplicate
                        .select(target.function, target.instruction)
                        .unwrap();
                }
            }
            assert!(duplicate.finish().is_err());
            let clone = first.instruction.clone();
            let mut foreign = plan.emission();
            assert!(foreign.select(first.function, &clone).is_err());
            for target in &plan.targets {
                foreign.select(target.function, target.instruction).unwrap();
            }
            assert!(foreign.finish().is_err());
            let mut wrong_function = plan.emission();
            assert!(
                wrong_function
                    .select(plan.root(), first.instruction)
                    .is_err()
            );
            for target in &plan.targets {
                wrong_function
                    .select(target.function, target.instruction)
                    .unwrap();
            }
            assert!(wrong_function.finish().is_err());
        }
    }
}
