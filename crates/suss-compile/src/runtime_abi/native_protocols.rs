//! Original native protocol fallback: tables belong to current callable values.
use super::*;
fn error(body: &mut Vec<Instruction<'static>>) {
    nominal::error(body);
}
fn cast_array(body: &mut Vec<Instruction<'static>>, local: u32) {
    body.extend([
        Instruction::LocalGet(local),
        Instruction::RefCastNonNull(HeapType::Concrete(ARGS)),
    ]);
}
fn key(body: &mut Vec<Instruction<'static>>, index: i32) {
    use Instruction::*;
    cast_array(body, 0);
    body.extend([I32Const(index), ArrayGet(ARGS)]);
}
/// `return_call_ref` of the closure in `local`'s entry for k operands, with
/// operands 1..=k.
fn tail_entry(body: &mut Vec<Instruction<'static>>, local: u32, k: u32) {
    use Instruction::*;
    body.extend([LocalGet(local), RefCastNonNull(HeapType::Concrete(CLOSURE))]);
    body.extend((1..=k).map(LocalGet));
    body.extend([
        LocalGet(local),
        RefCastNonNull(HeapType::Concrete(CLOSURE)),
        StructGet { struct_type_index: CLOSURE, field_index: CLOSURE_CODE },
        StructGet { struct_type_index: CODE, field_index: CODE_ENTRY + k },
        ReturnCallRef(CALL + k),
    ]);
}
fn callback(b: &mut Builder, body: &[Instruction<'static>]) -> u32 {
    let index = b.count;
    b.functions.function(INVOKE);
    let mut function = Function::new([(3, VALUE)]);
    for instruction in body {
        function.instruction(instruction);
    }
    function.instruction(&Instruction::End);
    b.code.function(&function);
    b.count += 1;
    index
}

pub(super) fn functions(
    b: &mut Builder,
    invoke: u32,
    method_get: u32,
    descriptor: u32,
) -> Vec<u32> {
    use Instruction::*;
    let mut body = vec![];
    for sentinel in [NIL, UNDEFINED] {
        body.extend([
            LocalGet(0),
            sentinel_value(sentinel),
            RefEq,
            If(BlockType::Empty),
            I32Const(0),
            Return,
            End,
        ]);
    }
    for sentinel in [FALSE, TRUE] {
        body.extend([
            LocalGet(0),
            sentinel_value(sentinel),
            RefEq,
            If(BlockType::Empty),
            I32Const(1),
            Return,
            End,
        ]);
    }
    body.extend([
        LocalGet(0),
        Call(b.names["source-array?"]),
        If(BlockType::Empty),
        I32Const(6),
        Return,
        End,
    ]);
    for (ty, kind) in [(NUMBER, 2), (STRING, 3), (4, 4), (ARGS, 6)] {
        body.extend([
            LocalGet(0),
            // A long is a number too.
            if ty == NUMBER {
                Call(b.names["number-test"])
            } else {
                RefTestNonNull(HeapType::Concrete(ty))
            },
            If(BlockType::Empty),
            I32Const(kind),
            Return,
            End,
        ]);
    }
    body.push(I32Const(5));
    let kind = b.function("protocol-native-kind", &[VALUE], &[ValType::I32], &body);
    // native-satisfies? is defined under *unchecked-if*: its property tests use
    // JS truthiness, while ordinary portable if keeps ClojureScript truthiness.
    let mut body = vec![];
    for sentinel in [NIL, FALSE, UNDEFINED] {
        body.extend([
            LocalGet(0),
            sentinel_value(sentinel),
            RefEq,
            If(BlockType::Empty),
            I32Const(0),
            Return,
            End,
        ]);
    }
    body.extend([
        LocalGet(0),
        Call(b.names["number-test"]),
        If(BlockType::Empty),
        LocalGet(0),
        Call(b.names["number-unbox"]),
        LocalSet(1),
        LocalGet(1),
        F64Const(0.0.into()),
        F64Ne,
        LocalGet(1),
        LocalGet(1),
        F64Eq,
        I32And,
        Return,
        End,
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(STRING)),
        If(BlockType::Empty),
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(STRING)),
        ArrayLen,
        I32Const(0),
        I32Ne,
        Return,
        End,
        I32Const(1),
    ]);
    let property_truthy = b.function_with_locals(
        "protocol-native-property-truthy",
        &[VALUE],
        &[ValType::I32],
        &[(1, ValType::F64)],
        &body,
    );
    let property_get = b.names["closure-property-get"];
    let property_set = b.names["closure-property-set"];
    let mut body = vec![
        LocalGet(0),
        LocalGet(1),
        sentinel_value(TRUE),
        Call(property_set),
        Drop,
        I32Const(1),
    ];
    b.function(
        "protocol-native-marker-set",
        &[VALUE, ValType::I32],
        &[ValType::I32],
        &body,
    );
    body = vec![
        LocalGet(2),
        RefTestNonNull(HeapType::Concrete(4)),
        I32Eqz,
        If(BlockType::Empty),
    ];
    error(&mut body);
    body.extend([
        End,
        LocalGet(0),
        LocalGet(1),
        LocalGet(2),
        Call(property_set),
    ]);
    b.function(
        "protocol-native-method-set",
        &[VALUE, ValType::I32, VALUE],
        &[VALUE],
        &body,
    );

    // A native implementation is a closure like any other (ADR-0004): it is
    // called with exactly the protocol call's operands, so a wrong count
    // reaches its arity error, as in Clojure. No operand is filled with
    // undefined or dropped.
    let mut body = vec![
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(CLOSURE)),
        I32Eqz,
        If(BlockType::Empty),
    ];
    error(&mut body);
    body.extend([
        End,
        LocalGet(1),
        RefTestNonNull(HeapType::Concrete(ARGS)),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    error(&mut body);
    body.extend([End, LocalGet(0), LocalGet(1), ReturnCall(invoke)]);
    let native_invoke = b.function_with_locals(
        "protocol-native-invoke",
        &[VALUE, VALUE],
        &[VALUE],
        &[(2, ValType::I32), (1, VALUE)],
        &body,
    );

    // A JavaScript-convention method call (`.method` interop, as the pinned
    // ClojureScript runs it): a function with one fixed arity receives exactly
    // that many operands, missing ones undefined and surplus ones dropped
    // (already evaluated). Other functions are called with their operands.
    let mut body = vec![
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(CLOSURE)),
        I32Eqz,
        If(BlockType::Empty),
    ];
    error(&mut body);
    body.extend([
        End,
        LocalGet(1),
        RefTestNonNull(HeapType::Concrete(ARGS)),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    error(&mut body);
    body.push(End);
    // The arity range: a universal closure's own, else its code struct's.
    body.extend([LocalGet(0), RefTestNonNull(HeapType::Concrete(UNIVERSAL)), If(BlockType::Empty)]);
    for (field, local) in [(UNIVERSAL_MIN, 2), (UNIVERSAL_MAX, 3)] {
        body.extend([
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(UNIVERSAL)),
            StructGet { struct_type_index: UNIVERSAL, field_index: field },
            LocalSet(local),
        ]);
    }
    body.push(Else);
    for (field, local) in [(CODE_MIN, 2), (CODE_MAX, 3)] {
        body.extend([
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(CLOSURE)),
            StructGet { struct_type_index: CLOSURE, field_index: CLOSURE_CODE },
            StructGet { struct_type_index: CODE, field_index: field },
            LocalSet(local),
        ]);
    }
    body.extend([
        End,
        LocalGet(2),
        LocalGet(3),
        I32Eq,
        If(BlockType::Empty),
        LocalGet(2),
        I32Const(0),
        I32LtS,
        If(BlockType::Empty),
    ]);
    error(&mut body);
    body.push(End);
    body.extend([
        LocalGet(1),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        ArrayLen,
        LocalSet(3),
        LocalGet(2),
        LocalGet(3),
        I32Ne,
        If(BlockType::Empty),
        sentinel_value(UNDEFINED),
        LocalGet(2),
        ArrayNew(ARGS),
        LocalSet(4),
        LocalGet(4),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        LocalGet(1),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        LocalGet(2),
        LocalGet(3),
        I32LtU,
        If(BlockType::Result(ValType::I32)),
        LocalGet(2),
        Else,
        LocalGet(3),
        End,
        ArrayCopy {
            array_type_index_dst: ARGS,
            array_type_index_src: ARGS,
        },
        LocalGet(0),
        LocalGet(4),
        Call(invoke),
        Return,
        End,
        End,
        LocalGet(0),
        LocalGet(1),
        Call(invoke),
    ]);
    b.function_with_locals(
        "js-method-invoke",
        &[VALUE, VALUE],
        &[VALUE],
        &[(2, ValType::I32), (1, VALUE)],
        &body,
    );

    // First direct object method, then CURRENT method's specific/default table.
    let mut body = vec![
        LocalGet(1),
        I32Const(0),
        ArrayGet(ARGS),
        LocalSet(2),
        LocalGet(2),
        RefTestNonNull(HeapType::Concrete(7)),
        If(BlockType::Empty),
        LocalGet(2),
        Call(descriptor),
    ];
    // Canonical IFn dispatchers carry an extra next-arity key. Other
    // protocols retain the ordinary receiver-inclusive source signature.
    cast_array(&mut body, 0);
    body.extend([ArrayLen, I32Const(3), I32Eq, If(BlockType::Result(VALUE))]);
    key(&mut body, 2);
    body.push(Else);
    key(&mut body, 0);
    body.push(End);
    body.extend([
        Call(method_get),
        LocalSet(3),
        LocalGet(3),
        RefTestNonNull(HeapType::Concrete(4)),
        If(BlockType::Empty),
        LocalGet(3),
    ]);
    cast_array(&mut body, 0);
    body.extend([
        ArrayLen,
        I32Const(3),
        I32Eq,
        If(BlockType::Result(reference(ARGS))),
        LocalGet(2),
        LocalGet(1),
        ArrayLen,
        I32Const(1),
        I32Add,
        ArrayNew(ARGS),
        LocalSet(5),
        LocalGet(5),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(1),
        LocalGet(1),
        I32Const(0),
        LocalGet(1),
        ArrayLen,
        ArrayCopy {
            array_type_index_dst: ARGS,
            array_type_index_src: ARGS,
        },
        LocalGet(5),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        Else,
        LocalGet(1),
        End,
    ]);
    body.push(Call(invoke));
    body.extend([Return, End, End]);
    key(&mut body, 1);
    body.extend([
        Call(b.names["binding-get"]),
        LocalSet(4),
        LocalGet(4),
        LocalGet(2),
        Call(kind),
        Call(property_get),
        LocalSet(3),
        LocalGet(3),
        sentinel_value(NIL),
        RefEq,
        LocalGet(3),
        sentinel_value(UNDEFINED),
        RefEq,
        I32Or,
        If(BlockType::Empty),
        LocalGet(4),
        I32Const(7),
        Call(property_get),
        LocalSet(3),
        End,
        LocalGet(3),
        RefTestNonNull(HeapType::Concrete(4)),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    error(&mut body);
    body.extend([End, LocalGet(3), LocalGet(1)]);
    body.push(Call(native_invoke));
    let dispatch = b.count;
    b.functions.function(INVOKE);
    let mut function = Function::new([(4, VALUE)]);
    for instruction in &body {
        function.instruction(instruction);
    }
    function.instruction(&End);
    b.code.function(&function);
    b.count += 1;
    // Typed dispatch entries (#355): entry k takes the dispatcher and the k
    // operands, finds the implementation as `dispatch` does, and calls its
    // entry for k operands directly, with no argument array or `invoke`.
    // Params: 0 dispatcher, 1..=k operands; locals: environment, then impl.
    for k in 1..=MAX_FIXED {
        let (environment, implementation) = (k + 1, k + 2);
        let mut body = vec![
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(UNIVERSAL)),
            StructGet { struct_type_index: UNIVERSAL, field_index: UNIVERSAL_MIN },
            I32Const(k as i32),
            I32Ne,
            If(BlockType::Empty),
            LocalGet(0),
            I32Const(k as i32),
            ReturnCall(b.names["arity-throw"]),
            End,
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(UNIVERSAL)),
            StructGet { struct_type_index: UNIVERSAL, field_index: UNIVERSAL_ENVIRONMENT },
            LocalSet(environment),
            LocalGet(1),
            RefTestNonNull(HeapType::Concrete(7)),
            If(BlockType::Empty),
        ];
        // The dispatcher caches its last descriptor and method, valid while
        // the protocol epoch is unchanged (#355): environment slots 2 to 4.
        body.extend([LocalGet(1), Call(descriptor)]);
        cast_array(&mut body, environment);
        body.extend([I32Const(2), ArrayGet(ARGS), RefEq]);
        cast_array(&mut body, environment);
        body.extend([
            I32Const(4),
            ArrayGet(ARGS),
            RefCastNonNull(HeapType::I31),
            I31GetS,
            GlobalGet(PROTOCOL_EPOCH_GLOBAL),
            I32Eq,
            I32And,
            If(BlockType::Result(VALUE)),
        ]);
        cast_array(&mut body, environment);
        body.extend([I32Const(3), ArrayGet(ARGS), Else, LocalGet(1), Call(descriptor)]);
        cast_array(&mut body, environment);
        body.extend([I32Const(0), ArrayGet(ARGS), Call(method_get), LocalSet(implementation)]);
        cast_array(&mut body, environment);
        body.extend([I32Const(2), LocalGet(1), Call(descriptor), ArraySet(ARGS)]);
        cast_array(&mut body, environment);
        body.extend([I32Const(3), LocalGet(implementation), ArraySet(ARGS)]);
        cast_array(&mut body, environment);
        body.extend([
            I32Const(4),
            GlobalGet(PROTOCOL_EPOCH_GLOBAL),
            I32Const(0x3fff_ffff),
            I32And,
            RefI31,
            ArraySet(ARGS),
            LocalGet(implementation),
            End,
            LocalTee(implementation),
            RefTestNonNull(HeapType::Concrete(CLOSURE)),
            If(BlockType::Empty),
        ]);
        tail_entry(&mut body, implementation, k);
        body.extend([End, End]);
        cast_array(&mut body, environment);
        body.extend([
            I32Const(1),
            ArrayGet(ARGS),
            Call(b.names["binding-get"]),
            LocalTee(environment),
            LocalGet(1),
            Call(kind),
            Call(property_get),
            LocalTee(implementation),
            sentinel_value(NIL),
            RefEq,
            LocalGet(implementation),
            sentinel_value(UNDEFINED),
            RefEq,
            I32Or,
            If(BlockType::Empty),
            LocalGet(environment),
            I32Const(7),
            Call(property_get),
            LocalSet(implementation),
            End,
            LocalGet(implementation),
            RefTestNonNull(HeapType::Concrete(CLOSURE)),
            I32Eqz,
            If(BlockType::Empty),
        ]);
        error(&mut body);
        body.push(End);
        tail_entry(&mut body, implementation, k);
        let index = b.typed(CALL + k, &[(2, VALUE)], &body);
        b.names.insert(format!("protocol-dispatch-{k}"), index);
    }
    for (name, count) in [
        ("protocol-live-dispatcher-new", 2u32),
        ("ifn-live-dispatcher-new", 3u32),
    ] {
        let arity_local = count;
        let schema_local = count + 1;
        body = vec![];
        if count == 3 {
            body.extend([
                LocalGet(2),
                RefTestNonNull(HeapType::Concrete(DESCRIPTOR)),
                I32Eqz,
                If(BlockType::Empty),
            ]);
            error(&mut body);
            body.push(End);
        }
        body.extend([
            LocalGet(0),
            RefTestNonNull(HeapType::Concrete(DESCRIPTOR)),
            I32Eqz,
            If(BlockType::Empty),
        ]);
        error(&mut body);
        body.push(End);
        body.extend([
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(DESCRIPTOR)),
            StructGet {
                struct_type_index: DESCRIPTOR,
                field_index: 1,
            },
            LocalSet(schema_local),
            LocalGet(schema_local),
            RefTestNonNull(HeapType::Concrete(ARGS)),
            I32Eqz,
            If(BlockType::Empty),
        ]);
        error(&mut body);
        body.extend([
            End,
            LocalGet(1),
            RefTestNonNull(HeapType::Concrete(5)),
            I32Eqz,
            If(BlockType::Empty),
        ]);
        error(&mut body);
        body.extend([
            End,
            LocalGet(schema_local),
            RefCastNonNull(HeapType::Concrete(ARGS)),
            ArrayLen,
            LocalSet(arity_local),
            LocalGet(arity_local),
            I32Eqz,
            If(BlockType::Empty),
        ]);
        error(&mut body);
        body.push(End);
        body.extend([LocalGet(0), LocalGet(1)]);
        if count == 3 {
            body.push(LocalGet(2));
        }
        if count == 3 {
            // IFn's dispatchers prepend the receiver; they keep `invoke`.
            body.extend([
                ArrayNewFixed {
                    array_type_index: ARGS,
                    array_size: count,
                },
                RefFunc(dispatch),
                LocalGet(arity_local),
                LocalGet(arity_local),
                Call(b.names["closure-new"]),
            ]);
        } else {
            // A universal closure whose code struct holds the typed dispatch
            // entries; `invoke` still reaches `dispatch` (#355).
            let operands = body.split_off(body.len() - 2);
            body.extend([GlobalGet(DISPATCH_CODE_GLOBAL), sentinel_value(NIL), I32Const(0), RefI31]);
            body.extend(operands);
            // Cache slots: descriptor, method, epoch (none yet).
            body.extend([sentinel_value(NIL), sentinel_value(NIL), I32Const(-1), RefI31]);
            body.extend([
                ArrayNewFixed {
                    array_type_index: ARGS,
                    array_size: count + 3,
                },
                RefFunc(dispatch),
                LocalGet(arity_local),
                LocalGet(arity_local),
                StructNew(UNIVERSAL),
            ]);
        }
        b.function_with_locals(
            name,
            &vec![VALUE; count as usize],
            &[VALUE],
            &[(1, ValType::I32), (1, VALUE)],
            &body,
        );
    }
    // The macro's object marker fast path remains separate. Native fallback
    // examines the current protocol value, not the stable syntactic marker.
    body = vec![LocalGet(0), RefIsNull, If(BlockType::Empty)];
    error(&mut body);
    body.push(End);
    for sentinel in [NIL, UNDEFINED] {
        body.extend([
            LocalGet(0),
            sentinel_value(sentinel),
            RefEq,
            If(BlockType::Empty),
        ]);
        error(&mut body);
        body.push(End);
    }
    body.extend([
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(7)),
        If(BlockType::Empty),
    ]);
    error(&mut body);
    body.push(End);
    body.extend([
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(4)),
        I32Eqz,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
        LocalGet(0),
        LocalGet(1),
        Call(kind),
        Call(property_get),
        LocalSet(2),
        LocalGet(2),
        Call(property_truthy),
        I32Eqz,
        If(BlockType::Empty),
        LocalGet(0),
        I32Const(7),
        Call(property_get),
        LocalSet(2),
        End,
        LocalGet(2),
        Call(property_truthy),
    ]);
    let satisfies = b.function_with_locals(
        "protocol-native-satisfies",
        &[VALUE, VALUE],
        &[ValType::I32],
        &[(1, VALUE)],
        &body,
    );
    let function_body = vec![
        LocalGet(1),
        I32Const(0),
        ArrayGet(ARGS),
        LocalGet(1),
        I32Const(1),
        ArrayGet(ARGS),
        Call(satisfies),
        If(BlockType::Result(VALUE)), GlobalGet(TRUE_GLOBAL), Else, GlobalGet(FALSE_GLOBAL), End,
    ];
    let function = callback(b, &function_body);
    b.function(
        "native-satisfies-function",
        &[],
        &[VALUE],
        &[
            sentinel_value(NIL),
            RefFunc(function),
            I32Const(2),
            I32Const(2),
            Call(b.names["closure-new"]),
        ],
    );
    // Original ordinary IFn call adapter. Capture the actual source method
    // before arguments execute, matching emitted .call evaluation. The receiver
    // and method remain rooted in a canonical closure environment; source MetaFn
    // still owns behavior. Explicit -invoke uses its separate native fallback.
    let mut code = vec![
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(ARGS)),
        I32Eqz,
        If(BlockType::Empty),
    ];
    error(&mut code);
    code.extend([
        End,
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        ArrayLen,
        I32Const(2),
        I32Ne,
        If(BlockType::Empty),
    ]);
    error(&mut code);
    code.extend([
        End,
        LocalGet(1),
        ArrayLen,
        I32Const(i32::MAX),
        I32GeU,
        If(BlockType::Empty),
    ]);
    error(&mut code);
    code.extend([
        End,
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        ArrayGet(ARGS),
        LocalGet(1),
        ArrayLen,
        I32Const(1),
        I32Add,
        ArrayNew(ARGS),
        LocalSet(2),
        LocalGet(2),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(1),
        LocalGet(1),
        I32Const(0),
        LocalGet(1),
        ArrayLen,
        ArrayCopy {
            array_type_index_dst: ARGS,
            array_type_index_src: ARGS,
        },
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(1),
        ArrayGet(ARGS),
        LocalGet(2),
        Call(invoke),
    ]);
    let bound = callback(b, &code);
    let mut code = vec![LocalGet(1), RefTestNonNull(HeapType::Concrete(DESCRIPTOR)), I32Eqz, If(BlockType::Empty)];
    error(&mut code);
    code.extend([End, LocalGet(0), RefTestNonNull(HeapType::Concrete(7)), If(BlockType::Result(VALUE)),
        LocalGet(0), Call(descriptor), LocalGet(1), Call(method_get), Else,
        // An unbound name stays undefined, so invoke reports Clojure's unbound-fn
        // error (#288); any other non-object has no method.
        LocalGet(0), sentinel_value(UNDEFINED), RefEq, If(BlockType::Result(VALUE)), LocalGet(0), Else,
        sentinel_value(NIL), End, End, LocalSet(2),
        LocalGet(0), LocalGet(2), ArrayNewFixed { array_type_index: ARGS, array_size: 2 },
        RefFunc(bound), I32Const(0), I32Const(-1), Call(b.names["closure-new"])]);
    b.function_with_locals("callable-bind", &[VALUE, VALUE], &[VALUE], &[(1, VALUE)], &code);
    vec![dispatch, function, bound]
}
