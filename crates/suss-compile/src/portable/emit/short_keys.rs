//! Admission proves a literal key has exactly one adjacent, same-block use.
use super::{ir, Diagnostic, IrFunction, Literal, Nominal, Operation, Terminator};
type Meter<'a> = dyn FnMut(u64) -> Result<(), Diagnostic> + 'a;
#[derive(Default)]
pub(super) struct Plan {
    literals: Vec<u8>,
}
pub(super) fn adjacent(pair: &[ir::Instruction]) -> Option<ir::ValueId> {
    match (&pair[0].operation, &pair[1].operation) {
        (
            Operation::Literal(Literal::String(units)),
            Operation::Nominal {
                operation: Nominal::NamedGet,
                arguments,
            },
        ) if units.len() <= 4 && arguments.len() == 2 && arguments[1] == pair[0].result => {
            Some(pair[0].result)
        }
        _ => None,
    }
}
pub(super) fn present(ir: &IrFunction, meter: &mut Meter<'_>) -> Result<bool, Diagnostic> {
    for block in &ir.blocks {
        meter(1)?;
        for pair in block.instructions.windows(2) {
            meter(1)?;
            if adjacent(pair).is_some() {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
impl Plan {
    pub fn new(ir: &IrFunction, enabled: bool, meter: &mut Meter<'_>) -> Result<Self, Diagnostic> {
        if !enabled || !present(ir, meter)? {
            return Ok(Self::default());
        }
        let fail = |message: &str| Diagnostic {
            span: ir.span.clone(),
            message: message.into(),
        };
        let len = ir.values.len();
        let work = len
            .checked_mul(6) // two byte backings, initialization and eventual retirement
            .and_then(|n| {
                n.checked_add(std::mem::size_of::<Self>() + std::mem::size_of::<Vec<u8>>() + 4)
            })
            .and_then(|n| u64::try_from(n).ok())
            .ok_or_else(|| fail("Short-key plan storage overflow"))?;
        meter(work)?;
        let mut uses = Vec::<u8>::new();
        uses.try_reserve_exact(len)
            .map_err(|_| fail("Unable to reserve short-key uses"))?;
        uses.resize(len, 0);
        let mut literals = Vec::<u8>::new();
        literals
            .try_reserve_exact(len)
            .map_err(|_| fail("Unable to reserve short-key plan"))?;
        literals.resize(len, 0);
        for block in &ir.blocks {
            meter(1)?;
            for inst in &block.instructions {
                meter(1)?;
                for operand in ir::operands(&inst.operation) {
                    meter(1)?;
                    uses[operand.0] = uses[operand.0].saturating_add(1).min(2);
                }
            }
            meter(1)?;
            let exits = match &block.terminator {
                Terminator::Return(v) | Terminator::Throw(v) => std::slice::from_ref(v),
                Terminator::Jump { arguments, .. } => arguments,
                Terminator::Branch { condition, .. } => std::slice::from_ref(condition),
            };
            for v in exits {
                meter(1)?;
                uses[v.0] = uses[v.0].saturating_add(1).min(2);
            }
        }
        for block in &ir.blocks {
            meter(1)?;
            for pair in block.instructions.windows(2) {
                meter(1)?;
                if let Some(id) = adjacent(pair) {
                    meter(2)?; // use read and prospective flag publication
                    if uses[id.0] == 1 {
                        literals[id.0] = 1;
                    }
                }
            }
        }
        Ok(Self { literals })
    }
    pub fn elide(&self, id: ir::ValueId) -> bool {
        self.literals.get(id.0).copied() == Some(1)
    }
}
pub(super) fn pack(units: &[u16]) -> i64 {
    units
        .iter()
        .enumerate()
        .fold(0u64, |bits, (i, u)| bits | (u64::from(*u) << (16 * i))) as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::hir::{Expression, Hir, Type};
    fn getter() -> IrFunction {
        let literal = |value, ty| Hir {
            source: None,
            span: 3..7,
            metadata: vec![],
            ty,
            kind: Expression::Literal(value),
        };
        ir::lower(&Hir {
            source: None,
            span: 3..7,
            metadata: vec![],
            ty: Type::Value,
            kind: Expression::Nominal {
                operation: Nominal::NamedGet,
                arguments: vec![
                    literal(Literal::Nil, Type::Nil),
                    literal(Literal::String(vec![110, 115]), Type::String),
                ],
            },
        })
        .unwrap()
    }
    #[test]
    fn short_key_planning_cancellation_is_typed_and_full_retry_preserves_input() {
        let ir = getter();
        let original = format!("{ir:?}");
        let mut charges = Vec::new();
        let full = Plan::new(&ir, true, &mut |n| {
            charges.push(n);
            Ok(())
        })
        .unwrap();
        let key = ir.blocks[0]
            .instructions
            .iter()
            .find(|i| matches!(&i.operation, Operation::Literal(Literal::String(_))))
            .unwrap()
            .result;
        assert!(full.elide(key));
        for fail_at in 0..charges.len() {
            let expected = Diagnostic {
                span: 3..7,
                message: format!("injected cancellation {fail_at}"),
            };
            let mut at = 0;
            let result = Plan::new(&ir, true, &mut |_| {
                let fail = at == fail_at;
                at += 1;
                if fail {
                    Err(Diagnostic {
                        span: expected.span.clone(),
                        message: expected.message.clone(),
                    })
                } else {
                    Ok(())
                }
            });
            let Err(actual) = result else {
                panic!("missing cancellation {fail_at}")
            };
            assert_eq!(actual.span, expected.span);
            assert_eq!(actual.message, expected.message);
            assert_eq!(format!("{ir:?}"), original);
            let mut retry = Vec::new();
            assert!(Plan::new(&ir, true, &mut |n| {
                retry.push(n);
                Ok(())
            })
            .unwrap()
            .elide(key));
            assert_eq!(retry, charges);
        }
    }
    #[test]
    fn short_key_continuation_disabled_and_nonadjacent_keys_never_elide() {
        // A module with no eligible getter passes disabled admission. Even an
        // immediately failing meter must not be called: no walk or scratch
        // acquisition occurs. The separate continuation case below uses a
        // candidate-bearing graph to prove suspension remains disabled too.
        let ordinary = ir::lower(&Hir {
            source: None,
            span: 3..7,
            metadata: vec![],
            ty: Type::Nil,
            kind: Expression::Literal(Literal::Nil),
        })
        .unwrap();
        let plan = Plan::new(&ordinary, false, &mut |_| {
            Err(Diagnostic {
                span: 3..7,
                message: "unexpected no-candidate planning/allocation".into(),
            })
        })
        .unwrap();
        assert_eq!(plan.literals.capacity(), 0);
        let mut ir = getter();
        let mut work = 0;
        let plan = Plan::new(&ir, false, &mut |n| {
            work += n;
            Ok(())
        })
        .unwrap();
        assert_eq!(work, 0, "continuations do not construct/use raw-key plans");
        assert!(ir.blocks[0]
            .instructions
            .iter()
            .all(|i| !plan.elide(i.result)));
        let id = ir::ValueId(ir.values.len());
        ir.values.push(ir::Value {
            ty: Type::Nil,
            span: 3..7,
        });
        let before_get = ir.blocks[0].instructions.len() - 1;
        ir.blocks[0].instructions.insert(
            before_get,
            ir::Instruction {
                result: id,
                span: 3..7,
                operation: Operation::Literal(Literal::Nil),
            },
        );
        let plan = Plan::new(&ir, true, &mut |_| Ok(())).unwrap();
        assert!(ir.blocks[0]
            .instructions
            .iter()
            .all(|i| !plan.elide(i.result)));
    }
}
