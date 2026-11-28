//! Synchronous exported borrows carry a token specific to their enclosing call.
use super::*;
#[derive(Clone, Copy)]
pub(in crate::portable::aot) struct Context {
    pub previous: u32,
    pub token: u32,
    pub error: u32,
    pub cursor: u32,
    pub state: u32,
    pub returned: Option<u32>,
}
/// Canonical lowering creates one borrowed table entry per incoming leaf. Keep
/// all entries until source invocation completes, including discarded values.
pub(super) fn register(carrier: &Carrier, body: &mut Function, state: u32) {
    use Instruction::*;
    emit(
        body,
        &[
            GlobalGet(carrier.borrow_scope),
            RefCastNonNull(HeapType::Concrete(carrier.types.token)),
            LocalGet(state),
            GlobalGet(carrier.borrow_scope),
            RefCastNonNull(HeapType::Concrete(carrier.types.token)),
            StructGet {
                struct_type_index: carrier.types.token,
                field_index: 1,
            },
            ArrayNewFixed {
                array_type_index: runtime_abi::ARGS,
                array_size: 2,
            },
            StructSet {
                struct_type_index: carrier.types.token,
                field_index: 1,
            },
        ],
    );
}
impl Carrier {
    pub(in crate::portable::aot) fn begin_borrow(&self, body: &mut Function, context: &Context) {
        use Instruction::*;
        emit(
            body,
            &[
                GlobalGet(self.borrow_scope),
                LocalSet(context.previous),
                I32Const(1),
                RefNull(HeapType::Abstract {
                    shared: false,
                    ty: AbstractHeapType::Eq,
                }),
                StructNew(self.types.token),
                LocalTee(context.token),
                GlobalSet(self.borrow_scope),
                Block(BlockType::Empty),
                Block(BlockType::Result(VALUE)),
                TryTable(
                    BlockType::Empty,
                    std::borrow::Cow::Owned(vec![wasm_encoder::Catch::One {
                        tag: self.helpers.error_tag,
                        label: 0,
                    }]),
                ),
            ],
        );
    }
    fn expire_borrow(&self, body: &mut Function, context: &Context) {
        use Instruction::*;
        emit(
            body,
            &[
                LocalGet(context.token),
                RefCastNonNull(HeapType::Concrete(self.types.token)),
                I32Const(0),
                StructSet {
                    struct_type_index: self.types.token,
                    field_index: 0,
                },
                LocalGet(context.previous),
                GlobalSet(self.borrow_scope),
                LocalGet(context.token),
                RefCastNonNull(HeapType::Concrete(self.types.token)),
                StructGet {
                    struct_type_index: self.types.token,
                    field_index: 1,
                },
                LocalSet(context.cursor),
                LocalGet(context.token),
                RefCastNonNull(HeapType::Concrete(self.types.token)),
                RefNull(HeapType::Abstract {
                    shared: false,
                    ty: AbstractHeapType::Eq,
                }),
                StructSet {
                    struct_type_index: self.types.token,
                    field_index: 1,
                },
                Block(BlockType::Empty),
                Loop(BlockType::Empty),
                LocalGet(context.cursor),
                RefIsNull,
                BrIf(1),
                LocalGet(context.cursor),
                RefCastNonNull(HeapType::Concrete(runtime_abi::ARGS)),
                I32Const(0),
                ArrayGet(runtime_abi::ARGS),
                LocalSet(context.state),
            ],
        );
        // resource.drop removes the borrowed canonical entry; it does not call
        // the host resource destructor or transfer ownership of the host rep.
        self.state(body, context.state);
        body.instruction(&StructGet {
            struct_type_index: self.types.state,
            field_index: HANDLE,
        });
        self.state(body, context.state);
        emit(
            body,
            &[
                StructGet {
                    struct_type_index: self.types.state,
                    field_index: DROP,
                },
                CallRef(self.types.drop),
                LocalGet(context.cursor),
                RefCastNonNull(HeapType::Concrete(runtime_abi::ARGS)),
                I32Const(1),
                ArrayGet(runtime_abi::ARGS),
                LocalSet(context.cursor),
                Br(0),
                End,
                End,
            ],
        );
    }
    /// Preserve native return bits and language payloads. All export lifting,
    /// source invocation and result lowering execute inside this handler.
    pub(in crate::portable::aot) fn finish_borrow(&self, body: &mut Function, context: &Context) {
        use Instruction::*;
        if let Some(local) = context.returned {
            body.instruction(&LocalSet(local));
        }
        emit(
            body,
            &[Br(2), End, Unreachable, End, LocalSet(context.error)],
        );
        self.expire_borrow(body, context);
        emit(
            body,
            &[LocalGet(context.error), Throw(self.helpers.error_tag), End],
        );
        self.expire_borrow(body, context);
        if let Some(local) = context.returned {
            body.instruction(&LocalGet(local));
        }
    }
}
