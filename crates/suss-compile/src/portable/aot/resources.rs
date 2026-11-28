//! Opaque source carriers for imported resources. Canonical indices stay private.
use crate::runtime_abi;
use wasm_encoder::*;

use super::VALUE;
mod scopes;
pub(super) const API_OPERATIONS: &[&str] = &["close!", "call-with-scope"];
const TYPE: u32 = 0;
const HANDLE: u32 = 1;
const STATUS: u32 = 2;
const OWNED: u32 = 3;
const DROP: u32 = 4;
const MOVED: i32 = 1;
const CLOSED: i32 = 2;

fn next_type(types: &TypeSection) -> u32 {
    types.len() + runtime_abi::TYPE_COUNT - 1
}
fn reference(index: u32) -> ValType {
    ValType::Ref(RefType {
        nullable: false,
        heap_type: HeapType::Concrete(index),
    })
}
#[derive(Clone, Copy)]
pub(super) struct Types {
    pub drop: u32,
    pub state: u32,
    pub make: u32,
    pub check: u32,
    pub close: u32,
    pub reserve: u32,
    pub commit: u32,
    pub gather: u32,
}
impl Types {
    /// Append after the adapter's fixed helper types; no shared ABI type changes.
    pub fn declare(types: &mut TypeSection) -> Self {
        let drop = next_type(types);
        types.ty().function([ValType::I32], []);
        let state = next_type(types);
        types.ty().struct_([
            FieldType {
                element_type: StorageType::Val(ValType::I32),
                mutable: false,
            },
            FieldType {
                element_type: StorageType::Val(ValType::I32),
                mutable: false,
            },
            FieldType {
                element_type: StorageType::Val(ValType::I32),
                mutable: true,
            },
            FieldType {
                element_type: StorageType::Val(ValType::I32),
                mutable: false,
            },
            FieldType {
                element_type: StorageType::Val(reference(drop)),
                mutable: false,
            },
            FieldType {
                element_type: StorageType::Val(VALUE),
                mutable: false,
            },
        ]);
        let make = next_type(types);
        types
            .ty()
            .function([ValType::I32, ValType::I32, reference(drop)], [VALUE]);
        let check = next_type(types);
        types
            .ty()
            .function([VALUE, ValType::I32, ValType::I32], [reference(state)]);
        let close = next_type(types);
        types.ty().function([VALUE], []);
        let reserve = next_type(types);
        types.ty().function([VALUE, ValType::I32, VALUE], [VALUE]);
        let commit = next_type(types);
        types.ty().function([VALUE], []);
        let gather = next_type(types);
        types.ty().function([VALUE, VALUE], [VALUE]);
        Self {
            drop,
            state,
            make,
            check,
            close,
            reserve,
            commit,
            gather,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct Helpers {
    pub object_new: u32,
    pub string_new: u32,
    pub string_set: u32,
    pub error_new: u32,
    pub error_tag: u32,
}
/// The descriptor is a closed, nonconstructible native class with one private
/// state slot. State is neither a source number nor a source-visible collection.
pub(super) struct Carrier {
    pub types: Types,
    pub helpers: Helpers,
    pub descriptor: u32,
    pub scope: u32,
    pub check_function: u32,
    pub close_function: u32,
}
fn emit(body: &mut Function, code: &[Instruction<'_>]) {
    for instruction in code {
        body.instruction(instruction);
    }
}
pub(super) fn string(body: &mut Function, helpers: Helpers, text: &str, scratch: u32) {
    use Instruction::*;
    let units = text.encode_utf16().collect::<Vec<_>>();
    emit(
        body,
        &[
            I32Const(units.len() as i32),
            Call(helpers.string_new),
            LocalSet(scratch),
        ],
    );
    for (index, unit) in units.into_iter().enumerate() {
        emit(
            body,
            &[
                LocalGet(scratch),
                I32Const(index as i32),
                I32Const(unit as i32),
                Call(helpers.string_set),
                Drop,
            ],
        );
    }
    body.instruction(&LocalGet(scratch));
}
impl Carrier {
    fn fail(&self, body: &mut Function, message: &str) {
        string(body, self.helpers, message, 5);
        emit(
            body,
            &[
                Instruction::Call(self.helpers.error_new),
                Instruction::Throw(self.helpers.error_tag),
            ],
        );
    }
    fn require(&self, body: &mut Function, message: &str) {
        use Instruction::*;
        emit(body, &[I32Eqz, If(BlockType::Empty)]);
        self.fail(body, message);
        body.instruction(&End);
    }
    fn state(&self, body: &mut Function, local: u32) {
        emit(
            body,
            &[
                Instruction::LocalGet(local),
                Instruction::RefCastNonNull(HeapType::Concrete(self.types.state)),
            ],
        );
    }
    pub fn make(&self) -> Function {
        use Instruction::*;
        let mut body = Function::new([(2, VALUE)]);
        emit(
            &mut body,
            &[
                GlobalGet(self.descriptor),
                LocalGet(1),
                LocalGet(0),
                I32Const(0),
                I32Const(1),
                LocalGet(2),
                GlobalGet(self.scope),
                StructNew(self.types.state),
                LocalTee(3),
                ArrayNewFixed {
                    array_type_index: runtime_abi::ARGS,
                    array_size: 1,
                },
                Call(self.helpers.object_new),
                LocalSet(4),
            ],
        );
        scopes::register(self, &mut body, 3);
        emit(&mut body, &[LocalGet(4), End]);
        body
    }
    /// Return the private state only after class, shape, type, status and owner
    /// checks. Failed validation throws a language exception before host calls.
    pub fn check(&self) -> Function {
        use Instruction::*;
        let mut body = Function::new([(3, VALUE)]);
        emit(
            &mut body,
            &[
                LocalGet(0),
                RefTestNonNull(HeapType::Concrete(runtime_abi::OBJECT)),
            ],
        );
        self.require(&mut body, "WIT resource requires an opaque handle");
        emit(
            &mut body,
            &[
                LocalGet(0),
                RefCastNonNull(HeapType::Concrete(runtime_abi::OBJECT)),
                StructGet {
                    struct_type_index: runtime_abi::OBJECT,
                    field_index: 0,
                },
                GlobalGet(self.descriptor),
                RefEq,
            ],
        );
        self.require(&mut body, "WIT resource requires an opaque handle");
        emit(
            &mut body,
            &[
                LocalGet(0),
                RefCastNonNull(HeapType::Concrete(runtime_abi::OBJECT)),
                StructGet {
                    struct_type_index: runtime_abi::OBJECT,
                    field_index: 1,
                },
                LocalTee(3),
                RefTestNonNull(HeapType::Concrete(runtime_abi::ARGS)),
            ],
        );
        self.require(&mut body, "WIT resource handle storage is invalid");
        emit(
            &mut body,
            &[
                LocalGet(3),
                RefCastNonNull(HeapType::Concrete(runtime_abi::ARGS)),
                ArrayLen,
                I32Const(1),
                I32Eq,
            ],
        );
        self.require(&mut body, "WIT resource handle storage is invalid");
        emit(
            &mut body,
            &[
                LocalGet(3),
                RefCastNonNull(HeapType::Concrete(runtime_abi::ARGS)),
                I32Const(0),
                ArrayGet(runtime_abi::ARGS),
                LocalTee(4),
                RefTestNonNull(HeapType::Concrete(self.types.state)),
            ],
        );
        self.require(&mut body, "WIT resource handle storage is invalid");
        emit(
            &mut body,
            &[LocalGet(1), I32Const(0), I32GeS, If(BlockType::Empty)],
        );
        self.state(&mut body, 4);
        emit(
            &mut body,
            &[
                StructGet {
                    struct_type_index: self.types.state,
                    field_index: TYPE,
                },
                LocalGet(1),
                I32Eq,
            ],
        );
        self.require(&mut body, "WIT resource handle has the wrong declared type");
        body.instruction(&End);
        self.state(&mut body, 4);
        emit(
            &mut body,
            &[
                StructGet {
                    struct_type_index: self.types.state,
                    field_index: STATUS,
                },
                I32Eqz,
            ],
        );
        self.require(&mut body, "WIT resource handle is closed or moved");
        emit(&mut body, &[LocalGet(2), If(BlockType::Empty)]);
        self.state(&mut body, 4);
        emit(
            &mut body,
            &[
                StructGet {
                    struct_type_index: self.types.state,
                    field_index: OWNED,
                },
                I32Const(1),
                I32Eq,
            ],
        );
        self.require(&mut body, "WIT resource operation requires ownership");
        body.instruction(&End);
        self.state(&mut body, 4);
        body.instruction(&End);
        body
    }
    pub fn close(&self) -> Function {
        use Instruction::*;
        let mut body = Function::new([(1, VALUE)]);
        emit(
            &mut body,
            &[
                LocalGet(0),
                I32Const(-1),
                I32Const(1),
                Call(self.check_function),
                LocalSet(1),
            ],
        );
        self.state(&mut body, 1);
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
        self.state(&mut body, 1);
        body.instruction(&StructGet {
            struct_type_index: self.types.state,
            field_index: HANDLE,
        });
        self.state(&mut body, 1);
        emit(
            &mut body,
            &[
                StructGet {
                    struct_type_index: self.types.state,
                    field_index: DROP,
                },
                CallRef(self.types.drop),
                End,
            ],
        );
        body
    }
    /// Collect a private [state, own?, next] entry. Repeated borrows are
    /// permitted; any repeated state involving an own is rejected atomically.
    pub fn reserve(&self) -> Function {
        use Instruction::*;
        let mut body = Function::new([(3, VALUE)]);
        emit(
            &mut body,
            &[
                LocalGet(2),
                LocalSet(3),
                Block(BlockType::Empty),
                Loop(BlockType::Empty),
                LocalGet(3),
                RefIsNull,
                BrIf(1),
                LocalGet(3),
                RefCastNonNull(HeapType::Concrete(runtime_abi::ARGS)),
                I32Const(0),
                ArrayGet(runtime_abi::ARGS),
                LocalGet(0),
                RefEq,
                If(BlockType::Empty),
                LocalGet(3),
                RefCastNonNull(HeapType::Concrete(runtime_abi::ARGS)),
                I32Const(1),
                ArrayGet(runtime_abi::ARGS),
                RefCastNonNull(HeapType::I31),
                I31GetU,
                LocalGet(1),
                I32Or,
                I32Eqz,
            ],
        );
        self.require(
            &mut body,
            "WIT resource arguments contain conflicting ownership aliases",
        );
        emit(
            &mut body,
            &[
                End,
                LocalGet(3),
                RefCastNonNull(HeapType::Concrete(runtime_abi::ARGS)),
                I32Const(2),
                ArrayGet(runtime_abi::ARGS),
                LocalSet(3),
                Br(0),
                End,
                End,
                LocalGet(0),
                LocalGet(1),
                RefI31,
                LocalGet(2),
                ArrayNewFixed {
                    array_type_index: runtime_abi::ARGS,
                    array_size: 3,
                },
                End,
            ],
        );
        body
    }
    /// Revalidate every state before changing any status. There are no guest
    /// or host calls between this check and the complete move commit.
    pub fn commit(&self) -> Function {
        use Instruction::*;
        let mut body = Function::new([(5, VALUE)]);
        for commit in [false, true] {
            emit(
                &mut body,
                &[
                    LocalGet(0),
                    LocalSet(1),
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
            if commit {
                emit(
                    &mut body,
                    &[
                        LocalGet(1),
                        RefCastNonNull(HeapType::Concrete(runtime_abi::ARGS)),
                        I32Const(1),
                        ArrayGet(runtime_abi::ARGS),
                        RefCastNonNull(HeapType::I31),
                        I31GetU,
                        If(BlockType::Empty),
                    ],
                );
                self.state(&mut body, 2);
                emit(
                    &mut body,
                    &[
                        I32Const(MOVED),
                        StructSet {
                            struct_type_index: self.types.state,
                            field_index: STATUS,
                        },
                        End,
                    ],
                );
            } else {
                self.state(&mut body, 2);
                emit(
                    &mut body,
                    &[
                        StructGet {
                            struct_type_index: self.types.state,
                            field_index: STATUS,
                        },
                        I32Eqz,
                    ],
                );
                self.require(&mut body, "WIT resource handle is closed or moved");
            }
            emit(
                &mut body,
                &[
                    LocalGet(1),
                    RefCastNonNull(HeapType::Concrete(runtime_abi::ARGS)),
                    I32Const(2),
                    ArrayGet(runtime_abi::ARGS),
                    LocalSet(1),
                    Br(0),
                    End,
                    End,
                ],
            );
        }
        body.instruction(&End);
        body
    }
    pub fn invoke_close(&self) -> Function {
        use Instruction::*;
        let mut body = Function::new([]);
        emit(
            &mut body,
            &[
                LocalGet(1),
                RefCastNonNull(HeapType::Concrete(runtime_abi::ARGS)),
                I32Const(0),
                ArrayGet(runtime_abi::ARGS),
                Call(self.close_function),
                runtime_abi::sentinel_value(runtime_abi::NIL),
                End,
            ],
        );
        body
    }
}

#[cfg(all(test, suss_host, not(suss_generator)))]
mod tests;

mod plan;
pub(super) use plan::Plan;

/// Data codecs validate handles and write raw indices without changing state.
/// The enclosing invoker commits complete ownership transfers before handoff.
pub(super) struct Codec<'a> {
    pub plan: &'a Plan,
    pub resolve: &'a wit_parser::Resolve,
    pub drop_calls: &'a [u32],
    pub make_function: u32,
    pub check_function: u32,
    pub state_type: u32,
}
impl Codec<'_> {
    fn ordinal(&self, id: wit_parser::TypeId) -> u32 {
        self.plan
            .ordinal(self.resolve, id)
            .expect("validated resource domain")
    }
    pub fn copy(&self, body: &mut Function, id: wit_parser::TypeId, owned: bool) {
        use Instruction::*;
        if !owned {
            body.instruction(&Unreachable);
            return;
        }
        let ordinal = self.ordinal(id);
        emit(
            body,
            &[
                LocalGet(0),
                I32Load(MemArg {
                    offset: 0,
                    align: 2,
                    memory_index: 0,
                }),
                I32Const(ordinal as i32),
                RefFunc(self.drop_calls[ordinal as usize]),
                Call(self.make_function),
            ],
        );
    }
    pub fn validate(&self, body: &mut Function, id: wit_parser::TypeId, owned: bool) {
        use Instruction::*;
        emit(
            body,
            &[
                LocalGet(0),
                I32Const(self.ordinal(id) as i32),
                I32Const(i32::from(owned)),
                Call(self.check_function),
                Drop,
            ],
        );
    }
    pub fn gather(&self, body: &mut Function, id: wit_parser::TypeId, owned: bool, reserve: u32) {
        use Instruction::*;
        emit(
            body,
            &[
                LocalGet(0),
                I32Const(self.ordinal(id) as i32),
                I32Const(i32::from(owned)),
                Call(self.check_function),
                I32Const(i32::from(owned)),
                LocalGet(1),
                Call(reserve),
                LocalSet(1),
            ],
        );
    }
    pub fn write(&self, body: &mut Function, id: wit_parser::TypeId, owned: bool) {
        use Instruction::*;
        emit(
            body,
            &[
                LocalGet(1),
                LocalGet(0),
                I32Const(self.ordinal(id) as i32),
                I32Const(i32::from(owned)),
                Call(self.check_function),
                StructGet {
                    struct_type_index: self.state_type,
                    field_index: HANDLE,
                },
                I32Store(MemArg {
                    offset: 0,
                    align: 2,
                    memory_index: 0,
                }),
            ],
        );
    }
}

pub(super) fn drop_name(index: usize) -> String {
    format!("{}suss.resource.drop.{index}", char::from(0_u8))
}
pub(super) struct Wiring {
    pub descriptor_new: u32,
    pub object_new: u32,
    pub try_invoke: u32,
    pub drop_calls: Vec<u32>,
}
pub(super) fn wire(
    types: &mut TypeSection,
    imports: &mut ImportSection,
    first: u32,
    plan: &Plan,
) -> Wiring {
    for (name, params) in [
        ("descriptor-new-core", vec![VALUE; 3]),
        ("object-new", vec![VALUE; 2]),
        ("try-invoke", vec![VALUE; 3]),
    ] {
        let ty = next_type(types);
        types.ty().function(params, [VALUE]);
        imports.import("suss.runtime", name, EntityType::Function(ty));
    }
    let mut drop_calls = Vec::new();
    for index in 0..plan.resources.len() {
        let ty = next_type(types);
        types.ty().function([ValType::I32], []);
        imports.import("suss.host", &drop_name(index), EntityType::Function(ty));
        drop_calls.push(first + 3 + index as u32);
    }
    Wiring {
        descriptor_new: first,
        object_new: first + 1,
        try_invoke: first + 2,
        drop_calls,
    }
}
pub(super) fn initialize_descriptor(
    body: &mut Function,
    helpers: Helpers,
    descriptor_new: u32,
    descriptor: u32,
) {
    use Instruction::*;
    string(body, helpers, "resource-state", 0);
    body.instruction(&ArrayNewFixed {
        array_type_index: runtime_abi::ARGS,
        array_size: 1,
    });
    string(body, helpers, "suss.resource.Handle", 0);
    emit(
        body,
        &[
            I32Const(0),
            RefI31,
            Call(descriptor_new),
            GlobalSet(descriptor),
        ],
    );
}
pub(super) fn install_operation(
    body: &mut Function,
    cell: u32,
    invoker: u32,
    wiring: &super::imports::Wiring,
) {
    use Instruction::*;
    emit(
        body,
        &[
            GlobalGet(cell),
            runtime_abi::sentinel_value(runtime_abi::NIL),
            RefFunc(invoker),
            I32Const(1),
            I32Const(1),
            Call(wiring.closure_new),
            Call(wiring.binding_set),
            Drop,
        ],
    );
}
