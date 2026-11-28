//! Explicit synchronous owner scopes; the runtime's try-invoke supplies finally.
use super::*;

/// Register each new owner once. Scope lists hold private states, keeping them
/// alive even if source code discards every carrier before scope exit.
pub(super) fn register(carrier: &Carrier, body: &mut Function, state: u32) {
    use Instruction::*;
    emit(
        body,
        &[
            GlobalGet(carrier.scope),
            RefTestNonNull(HeapType::Concrete(runtime_abi::ARGS)),
            If(BlockType::Empty),
            GlobalGet(carrier.scope),
            RefCastNonNull(HeapType::Concrete(runtime_abi::ARGS)),
            I32Const(1),
            LocalGet(state),
            GlobalGet(carrier.scope),
            RefCastNonNull(HeapType::Concrete(runtime_abi::ARGS)),
            I32Const(1),
            ArrayGet(runtime_abi::ARGS),
            ArrayNewFixed {
                array_type_index: runtime_abi::ARGS,
                array_size: 2,
            },
            ArraySet(runtime_abi::ARGS),
            End,
        ],
    );
}
impl Carrier {
    /// Restore the outer scope before callbacks and sever frame links. Closed
    /// and moved owners are skipped; remaining owners close in reverse order.
    pub(in crate::portable::aot) fn cleanup_scope(&self) -> Function {
        use Instruction::*;
        let mut body = Function::new([(2, VALUE)]);
        emit(
            &mut body,
            &[
                LocalGet(0),
                RefCastNonNull(HeapType::Concrete(runtime_abi::ARGS)),
                I32Const(0),
                ArrayGet(runtime_abi::ARGS),
                GlobalSet(self.scope),
                LocalGet(0),
                RefCastNonNull(HeapType::Concrete(runtime_abi::ARGS)),
                I32Const(1),
                ArrayGet(runtime_abi::ARGS),
                LocalSet(1),
            ],
        );
        for index in [0, 1] {
            emit(
                &mut body,
                &[
                    LocalGet(0),
                    RefCastNonNull(HeapType::Concrete(runtime_abi::ARGS)),
                    I32Const(index),
                    RefNull(HeapType::Abstract {
                        shared: false,
                        ty: AbstractHeapType::Eq,
                    }),
                    ArraySet(runtime_abi::ARGS),
                ],
            );
        }
        emit(
            &mut body,
            &[
                Block(BlockType::Empty),
                Loop(BlockType::Empty),
                LocalGet(1),
                RefIsNull,
                BrIf(1),
                LocalGet(1),
                RefCastNonNull(HeapType::Concrete(runtime_abi::ARGS)),
                I32Const(0),
                ArrayGet(runtime_abi::ARGS),
                LocalSet(2),
            ],
        );
        self.state(&mut body, 2);
        emit(
            &mut body,
            &[
                StructGet {
                    struct_type_index: self.types.state,
                    field_index: STATUS,
                },
                I32Eqz,
                If(BlockType::Empty),
            ],
        );
        self.state(&mut body, 2);
        emit(
            &mut body,
            &[
                I32Const(CLOSED),
                StructSet {
                    struct_type_index: self.types.state,
                    field_index: STATUS,
                },
            ],
        );
        self.state(&mut body, 2);
        body.instruction(&StructGet {
            struct_type_index: self.types.state,
            field_index: HANDLE,
        });
        self.state(&mut body, 2);
        emit(
            &mut body,
            &[
                StructGet {
                    struct_type_index: self.types.state,
                    field_index: DROP,
                },
                CallRef(self.types.drop),
                End,
                LocalGet(1),
                RefCastNonNull(HeapType::Concrete(runtime_abi::ARGS)),
                I32Const(1),
                ArrayGet(runtime_abi::ARGS),
                LocalSet(1),
                Br(0),
                End,
                End,
                End,
            ],
        );
        body
    }
    pub(in crate::portable::aot) fn invoke_scope_cleanup(&self, cleanup: u32) -> Function {
        let mut body = Function::new([]);
        emit(
            &mut body,
            &[
                Instruction::LocalGet(0),
                Instruction::Call(cleanup),
                runtime_abi::sentinel_value(runtime_abi::NIL),
                Instruction::End,
            ],
        );
        body
    }
    /// Invoke a zero-arity body with the same language exception and cleanup
    /// rules as source try/finally. Canonical traps still require Store retirement.
    pub(in crate::portable::aot) fn invoke_scope(
        &self,
        cleanup_invoker: u32,
        try_invoke: u32,
        closure_new: u32,
    ) -> Function {
        use Instruction::*;
        let mut body = Function::new([(1, VALUE)]);
        emit(
            &mut body,
            &[
                GlobalGet(self.scope),
                RefNull(HeapType::Abstract {
                    shared: false,
                    ty: AbstractHeapType::Eq,
                }),
                ArrayNewFixed {
                    array_type_index: runtime_abi::ARGS,
                    array_size: 2,
                },
                LocalTee(2),
                GlobalSet(self.scope),
                LocalGet(1),
                RefCastNonNull(HeapType::Concrete(runtime_abi::ARGS)),
                I32Const(0),
                ArrayGet(runtime_abi::ARGS),
                runtime_abi::sentinel_value(runtime_abi::NIL),
                LocalGet(2),
                RefFunc(cleanup_invoker),
                I32Const(0),
                I32Const(0),
                Call(closure_new),
                Call(try_invoke),
                End,
            ],
        );
        body
    }
}
