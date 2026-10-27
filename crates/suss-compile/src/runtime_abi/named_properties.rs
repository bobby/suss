//! Original checked named storage for retained source; no host object registry.
use super::*;

fn array(body: &mut Vec<Instruction<'static>>, local: u32) {
    body.extend([
        Instruction::LocalGet(local),
        Instruction::RefCastNonNull(HeapType::Concrete(ARGS)),
    ]);
}
fn structure(body: &mut Vec<Instruction<'static>>, local: u32, ty: u32, index: u32) {
    body.extend([
        Instruction::LocalGet(local),
        Instruction::RefCastNonNull(HeapType::Concrete(ty)),
        Instruction::StructGet {
            struct_type_index: ty,
            field_index: index,
        },
    ]);
}
fn guard(body: &mut Vec<Instruction<'static>>, local: u32, ty: u32) {
    body.extend([
        Instruction::LocalGet(local),
        Instruction::RefTestNonNull(HeapType::Concrete(ty)),
        Instruction::I32Eqz,
        Instruction::If(BlockType::Empty),
    ]);
    nominal::error(body);
    body.push(Instruction::End);
}
fn undefined(body: &mut Vec<Instruction<'static>>) {
    body.extend([Instruction::I32Const(UNDEFINED), Instruction::RefI31]);
}
fn name(body: &mut Vec<Instruction<'static>>, value: &str) {
    let units: Vec<_> = value.encode_utf16().collect();
    body.extend(units.iter().map(|unit| Instruction::I32Const(*unit as i32)));
    body.push(Instruction::ArrayNewFixed {
        array_type_index: STRING,
        array_size: units.len() as u32,
    });
}

const INHERITED_NAMES: &[&str] = &[
    "constructor",
    "__proto__",
    "toString",
    "toLocaleString",
    "valueOf",
    "hasOwnProperty",
    "isPrototypeOf",
    "propertyIsEnumerable",
    "__defineGetter__",
    "__defineSetter__",
    "__lookupGetter__",
    "__lookupSetter__",
];
fn reject_host_names(body: &mut Vec<Instruction<'static>>, equal: u32, names: &[&str]) {
    use Instruction::*;
    for spelling in names {
        body.push(LocalGet(1));
        name(body, spelling);
        body.extend([Call(equal), If(BlockType::Empty)]);
        nominal::error(body);
        body.push(End);
    }
}

pub(super) fn functions(b: &mut Builder) -> Vec<u32> {
    use Instruction::*;
    // UTF-16 equality stays separate from the explicit native-name normalization.
    let mut body = vec![
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(STRING)),
        I32Eqz,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
    ];
    guard(&mut body, 1, STRING);
    for local in [0, 1] {
        body.extend([
            LocalGet(local),
            RefCastNonNull(HeapType::Concrete(STRING)),
            ArrayLen,
        ]);
        if local == 0 {
            body.push(LocalTee(3));
        }
    }
    body.extend([
        I32Ne,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
    ]);
    // Short property names avoid loop bookkeeping. Both operand guards and the
    // length comparison above still run before any unit access; no callback
    // occurs, and longer strings continue through the original live-unit loop.
    for index in 0..4 {
        body.extend([LocalGet(3), I32Const(index), I32LeU, If(BlockType::Empty), I32Const(1), Return, End]);
        for local in [0, 1] {
            body.extend([LocalGet(local), RefCastNonNull(HeapType::Concrete(STRING)), I32Const(index), ArrayGetU(STRING)]);
        }
        body.extend([I32Ne, If(BlockType::Empty), I32Const(0), Return, End]);
    }
    body.extend([
        I32Const(4),
        LocalSet(2),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(2),
        LocalGet(3),
        I32GeU,
        BrIf(1),
    ]);
    for local in [0, 1] {
        body.extend([
            LocalGet(local),
            RefCastNonNull(HeapType::Concrete(STRING)),
            LocalGet(2),
            ArrayGetU(STRING),
        ]);
    }
    body.extend([
        I32Ne,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
        LocalGet(2),
        I32Const(1),
        I32Add,
        LocalSet(2),
        Br(0),
        End,
        End,
        I32Const(1),
    ]);
    let equal = b.function_with_locals(
        "property-key-equal",
        &[VALUE, VALUE],
        &[ValType::I32],
        &[(2, ValType::I32)],
        &body,
    );

    let mut callbacks = object_methods::functions(b, equal);
    callbacks.extend(string_methods::functions(b));
    callbacks.extend(string_char_at::functions(b));
    callbacks.extend(array_methods::functions(b));
    callbacks.extend(array_push::functions(b));
    callbacks.extend(array_pop::functions(b));
    callbacks.extend(array_shift::functions(b));
    callbacks.extend(array_own_properties::functions(b, equal));
    callbacks.extend(array_join::functions(b));
    let (call, apply) = closure_calls::functions(b);
    callbacks.extend([call, apply]);

    // Public JS native names and internal kind keys name the same owned entry.
    let mut body = vec![];
    guard(&mut body, 0, STRING);
    body.extend([
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(STRING)),
        ArrayLen,
        LocalSet(1),
    ]);
    let mut native_names = std::collections::BTreeMap::<usize, Vec<(usize, &str)>>::new();
    for (kind, spelling) in [
        "null", "boolean", "number", "string", "function", "object", "array", "_",
    ]
    .into_iter()
    .enumerate()
    {
        native_names.entry(spelling.len()).or_default().push((kind, spelling));
    }
    // These closed ASCII spellings need no temporary GC strings. Length is
    // immutable; read the live UTF-16 units on every call, without callbacks.
    for (length, spellings) in native_names {
        body.extend([
            LocalGet(1),
            I32Const(length as i32),
            I32Eq,
            If(BlockType::Empty),
        ]);
        for (kind, spelling) in spellings {
            body.push(I32Const(1));
            for (index, unit) in spelling.encode_utf16().enumerate() {
                body.extend([
                    LocalGet(0),
                    RefCastNonNull(HeapType::Concrete(STRING)),
                    I32Const(index as i32),
                    ArrayGetU(STRING),
                    I32Const(i32::from(unit)),
                    I32Eq,
                    I32And,
                ]);
            }
            body.extend([
                If(BlockType::Empty),
                I32Const(kind as i32),
                RefI31,
                Return,
                End,
            ]);
        }
        body.push(End);
    }
    body.push(LocalGet(0));
    let normalize = b.function_with_locals(
        "property-native-name", &[VALUE], &[VALUE], &[(1, ValType::I32)], &body,
    );

    // Named instance access must not mistake raw source names for munged fields.
    // Boundary spellings checked against cljs/analyzer.cljc js-reserved (line190)
    // and cljs/compiler.cljc munge at the pinned c4295f303100bbf5afac449242d30bca1126f1a1.
    // This original guard rejects the unsupported adaptation instead of porting it.
    let mut body = vec![];
    guard(&mut body, 0, STRING);
    // Length is immutable and no callback occurs during this validation.
    body.extend([
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(STRING)),
        ArrayLen,
        LocalSet(3),
        I32Const(0),
        LocalSet(1),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(1),
        LocalGet(3),
        I32GeU,
        BrIf(1),
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(STRING)),
        LocalGet(1),
        ArrayGetU(STRING),
        LocalSet(2),
    ]);
    // Exactly the existing ASCII whitelist, represented by two word masks.
    // Reject all non-ASCII units before Wasm's modulo-64 shift semantics apply.
    let mut allowed = [0_u64; 2];
    for unit in b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_$" {
        allowed[usize::from(*unit) / 64] |= 1_u64 << (*unit % 64);
    }
    body.extend([
        // Lowercase letters dominate source field names. This only skips the
        // equivalent character mask, never the later reserved-spelling check.
        LocalGet(2),
        I32Const(i32::from(b'a')),
        I32Sub,
        I32Const(25),
        I32LeU,
        If(BlockType::Empty),
        Else,
        LocalGet(2),
        I32Const(128),
        I32GeU,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
        I64Const(allowed[0] as i64),
        I64Const(allowed[1] as i64),
        LocalGet(2),
        I32Const(64),
        I32LtU,
        Select,
        LocalGet(2),
        I64ExtendI32U,
        I64ShrU,
        I64Const(1),
        I64And,
        I64Eqz,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
        End,
        LocalGet(1),
        I32Const(1),
        I32Add,
        LocalSet(1),
        Br(0),
        End,
        End,
    ]);
    let reserved_names = [
        "arguments",
        "abstract",
        "await",
        "boolean",
        "break",
        "byte",
        "case",
        "catch",
        "char",
        "class",
        "const",
        "continue",
        "debugger",
        "default",
        "delete",
        "do",
        "double",
        "else",
        "enum",
        "export",
        "extends",
        "final",
        "finally",
        "float",
        "for",
        "function",
        "goto",
        "if",
        "implements",
        "import",
        "in",
        "instanceof",
        "int",
        "interface",
        "let",
        "long",
        "native",
        "new",
        "package",
        "private",
        "protected",
        "public",
        "return",
        "short",
        "static",
        "super",
        "switch",
        "synchronized",
        "this",
        "throw",
        "throws",
        "transient",
        "try",
        "typeof",
        "var",
        "void",
        "volatile",
        "while",
        "with",
        "yield",
        "methods",
        "null",
        "constructor",
        "__proto__",
    ];
    // Schema reads validate every field. Compare only reserved spellings of the
    // same length/first unit. Compare literal units without allocating temporary
    // strings; every spelling and the preceding full character guard remain.
    let mut by_length = std::collections::BTreeMap::<usize, Vec<&str>>::new();
    for reserved in reserved_names {
        by_length.entry(reserved.len()).or_default().push(reserved);
    }
    body.extend([
        LocalGet(3),
        I32Eqz,
        If(BlockType::Empty),
        I32Const(1),
        Return,
        End,
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(STRING)),
        I32Const(0),
        ArrayGetU(STRING),
        LocalSet(2),
    ]);
    for (length, names) in by_length {
        body.extend([
            LocalGet(3),
            I32Const(length as i32),
            I32Eq,
            If(BlockType::Empty),
        ]);
        let mut by_first = std::collections::BTreeMap::<u8, Vec<&str>>::new();
        for reserved in names {
            by_first
                .entry(reserved.as_bytes()[0])
                .or_default()
                .push(reserved);
        }
        for (first, spellings) in by_first {
            body.extend([
                LocalGet(2),
                I32Const(i32::from(first)),
                I32Eq,
                If(BlockType::Empty),
            ]);
            for reserved in spellings {
                body.push(I32Const(1));
                for (index, unit) in reserved.encode_utf16().enumerate().skip(1) {
                    body.extend([
                        LocalGet(0),
                        RefCastNonNull(HeapType::Concrete(STRING)),
                        I32Const(index as i32),
                        ArrayGetU(STRING),
                        I32Const(i32::from(unit)),
                        I32Eq,
                        I32And,
                    ]);
                }
                body.extend([If(BlockType::Empty), I32Const(0), Return, End]);
            }
            body.push(End);
        }
        // Length buckets are disjoint. After every reserved spelling in this
        // bucket has failed, no later bucket can reject this fully checked name.
        body.extend([I32Const(1), Return, End]);
    }
    body.push(I32Const(1));
    let field_supported = b.function_with_locals(
        "property-field-name-supported",
        &[VALUE],
        &[ValType::I32],
        &[(3, ValType::I32)],
        &body,
    );

    // Stride one is a field schema, stride two a mixed native/name table.
    let mut body = vec![];
    guard(&mut body, 0, ARGS);
    body.extend([LocalGet(2), I32Const(1), I32Eq, If(BlockType::Empty)]);
    guard(&mut body, 1, STRING);
    body.extend([
        Else,
        LocalGet(1),
        RefTestNonNull(HeapType::Concrete(STRING)),
        I32Eqz,
        If(BlockType::Empty),
        LocalGet(1),
        RefTestNonNull(HeapType::I31),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut body);
    body.extend([
        End,
        LocalGet(1),
        RefCastNonNull(HeapType::I31),
        I31GetU,
        // Closure tables also carry the private `prototype` and static-init
        // slots (8 and 9); source property names cannot address those keys.
        I32Const(9),
        I32GtU,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut body);
    body.extend([End, End, End]);
    body.extend([
        LocalGet(2),
        I32Const(1),
        I32Eq,
        LocalGet(2),
        I32Const(2),
        I32Eq,
        I32Or,
        I32Eqz,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut body);
    body.push(End);
    array(&mut body, 0);
    body.extend([ArrayLen, LocalTee(6), LocalGet(2), I32RemU, If(BlockType::Empty)]);
    nominal::error(&mut body);
    body.extend([
        End,
        I32Const(-1),
        LocalSet(5),
        // Field schemas have already established a STRING lookup key above.
        // Keep full per-entry validation, but avoid repeating stride dispatch,
        // key type tests, and identity/equality selection for every field.
        LocalGet(2),
        I32Const(1),
        I32Eq,
        If(BlockType::Empty),
        I32Const(0),
        LocalSet(3),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(3),
    ]);
    body.extend([LocalGet(6), I32GeU, BrIf(1)]);
    array(&mut body, 0);
    body.extend([LocalGet(3), ArrayGet(ARGS), LocalSet(4)]);
    guard(&mut body, 4, STRING);
    body.extend([
        LocalGet(4),
        Call(field_supported),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut body);
    body.extend([
        End,
        LocalGet(4),
        LocalGet(1),
        Call(equal),
        If(BlockType::Empty),
        LocalGet(3),
        LocalSet(5),
        End,
        LocalGet(3),
        I32Const(1),
        I32Add,
        LocalSet(3),
        Br(0),
        End,
        End,
        Else,
        // Stride-two tables retain native-key validation and first-match
        // behavior. This is the former mixed-table scan without field branches.
        I32Const(0),
        LocalSet(3),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(3),
    ]);
    body.extend([LocalGet(6), I32GeU, BrIf(1)]);
    array(&mut body, 0);
    body.extend([
        LocalGet(3),
        ArrayGet(ARGS),
        LocalSet(4),
        LocalGet(4),
        RefTestNonNull(HeapType::Concrete(STRING)),
        I32Eqz,
        If(BlockType::Empty),
        LocalGet(4),
        RefTestNonNull(HeapType::I31),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut body);
    body.extend([
        End,
        LocalGet(4),
        RefCastNonNull(HeapType::I31),
        I31GetU,
        I32Const(9),
        I32GtU,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut body);
    body.extend([
        End,
        End,
        LocalGet(4),
        LocalGet(1),
        RefEq,
        LocalGet(1),
        RefTestNonNull(HeapType::Concrete(STRING)),
        If(BlockType::Result(ValType::I32)),
        LocalGet(4),
        LocalGet(1),
        Call(equal),
        Else,
        I32Const(0),
        End,
        I32Or,
        If(BlockType::Empty),
        LocalGet(5),
        I32Const(0),
        I32LtS,
        If(BlockType::Empty),
        LocalGet(3),
        LocalSet(5),
        End,
        End,
        LocalGet(3),
        I32Const(2),
        I32Add,
        LocalSet(3),
        Br(0),
        End,
        End,
        End,
        LocalGet(5),
    ]);
    let find = b.function_with_locals(
        "property-find",
        &[VALUE, VALUE, ValType::I32],
        &[ValType::I32],
        &[(1, ValType::I32), (1, VALUE), (2, ValType::I32)],
        &body,
    );

    // Static members of user-facing collections that user code may read.
    const PUBLIC_STATICS: &[&str] = &[
        "EMPTY", "fromArray", "fromArrays", "createWithCheck", "createAsIfByAssoc", "HASHMAP_THRESHOLD",
    ];
    // Unprivileged named access (owner, name) -> owner (design decision
    // 2026-10-25). Fields of built-in instances are private to suss.core while
    // their methods stay public, as in Clojure. Only user-facing collection
    // classes expose whitelisted static reads; node classes, prototypes and
    // other statics stay private, and no built-in class accepts writes.
    let flag = |body: &mut Vec<Instruction<'static>>, descriptor: u32, bit: i64| {
        body.extend([LocalGet(descriptor), RefTestNonNull(HeapType::Concrete(DESCRIPTOR)), If(BlockType::Result(ValType::I32)),
            LocalGet(descriptor), RefCastNonNull(HeapType::Concrete(DESCRIPTOR)),
            StructGet { struct_type_index: DESCRIPTOR, field_index: 0 },
            I64Const(bit), I64And, I64Const(0), I64Ne,
            Else, I32Const(0), End]);
    };
    let private = |body: &mut Vec<Instruction<'static>>, descriptor: u32| flag(body, descriptor, nominal::CORE_PRIVATE_BIT);
    // Instances of runtime-built descriptors (ExceptionInfo) have identities
    // below the first counted one; their fields are public reads but never
    // writable, like Clojure's public final exception fields. Their classes
    // keep the host's read-only property semantics.
    let runtime_built = |body: &mut Vec<Instruction<'static>>, descriptor: u32| {
        body.extend([LocalGet(descriptor), RefTestNonNull(HeapType::Concrete(DESCRIPTOR)), If(BlockType::Result(ValType::I32)),
            LocalGet(descriptor), RefCastNonNull(HeapType::Concrete(DESCRIPTOR)),
            StructGet { struct_type_index: DESCRIPTOR, field_index: 0 },
            I64Const(nominal::CORE_CLOSED_BIT - 1), I64And, I64Const(nominal::FIRST_COUNTED_IDENTITY), I64LtS,
            Else, I32Const(0), End]);
    };
    for writing in [false, true] {
        let mut body = vec![LocalGet(0), RefTestNonNull(HeapType::Concrete(7)), If(BlockType::Empty)];
        structure(&mut body, 0, 7, 0);
        body.push(LocalSet(2));
        private(&mut body, 2);
        if writing {
            runtime_built(&mut body, 2);
            body.push(I32Or);
        }
        body.extend([If(BlockType::Empty)]);
        body.extend([LocalGet(2), RefCastNonNull(HeapType::Concrete(DESCRIPTOR)), StructGet { struct_type_index: DESCRIPTOR, field_index: 1 }]);
        body.extend([LocalGet(1), I32Const(1), Call(find), I32Const(0), I32GeS, If(BlockType::Empty)]);
        nominal::message_error(&mut body, "Built-in type internals are private to suss.core");
        body.extend([End, End, End]);
        body.extend([LocalGet(0), RefTestNonNull(HeapType::Concrete(CLOSURE)), If(BlockType::Empty),
            LocalGet(0), Call(b.names["closure-environment"]), LocalSet(2)]);
        private(&mut body, 2);
        if !writing {
            // Allowed only for whitelisted statics of user-facing collections.
            flag(&mut body, 2, nominal::CORE_PUBLIC_STATICS_BIT);
            body.push(I32Const(0));
            for public in PUBLIC_STATICS {
                body.push(LocalGet(1));
                name(&mut body, public);
                body.extend([Call(equal), I32Or]);
            }
            body.extend([I32And, I32Eqz, I32And]);
        }
        body.extend([If(BlockType::Empty)]);
        nominal::message_error(&mut body, "Built-in type internals are private to suss.core");
        body.extend([End, End, LocalGet(0)]);
        b.function_with_locals(
            if writing { "builtin-internal-set-guard" } else { "builtin-internal-get-guard" },
            &[VALUE, VALUE], &[VALUE], &[(1, VALUE)], &body,
        );
    }
    // Unprivileged named access (owner, name, caller class or nil) -> owner, as
    // in Clojure (#278): fields are public and final unless declared mutable,
    // and mutable fields are private to their type. The caller is the class
    // value of the type whose inline method body contains the access.
    for writing in [false, true] {
        let mut body = vec![LocalGet(0), RefTestNonNull(HeapType::Concrete(7)), If(BlockType::Empty)];
        structure(&mut body, 0, 7, 0);
        body.push(LocalSet(3));
        body.extend([LocalGet(3), RefCastNonNull(HeapType::Concrete(DESCRIPTOR)),
            StructGet { struct_type_index: DESCRIPTOR, field_index: 5 }, LocalSet(4),
            LocalGet(4), RefTestNonNull(HeapType::Concrete(ARGS)), If(BlockType::Empty),
            LocalGet(4), LocalGet(1), I32Const(1), Call(find), I32Const(0), I32GeS, If(BlockType::Empty),
            LocalGet(2), RefTestNonNull(HeapType::Concrete(CLOSURE)), If(BlockType::Result(ValType::I32)),
            LocalGet(2), Call(b.names["closure-environment"]), LocalGet(3), RefEq,
            Else, I32Const(0), End, I32Eqz, If(BlockType::Empty)]);
        nominal::message_error(&mut body, "Mutable fields are private to their type");
        body.extend([End, LocalGet(0), Return, End, End]);
        if writing {
            body.extend([LocalGet(3), RefCastNonNull(HeapType::Concrete(DESCRIPTOR)),
                StructGet { struct_type_index: DESCRIPTOR, field_index: 1 },
                LocalGet(1), I32Const(1), Call(find), I32Const(0), I32GeS, If(BlockType::Empty)]);
            nominal::message_error(&mut body, "Cannot assign an immutable field");
            body.push(End);
        }
        body.extend([End, LocalGet(0)]);
        b.function_with_locals(
            if writing { "user-field-set-guard" } else { "user-field-get-guard" },
            &[VALUE, VALUE, VALUE], &[VALUE], &[(2, VALUE)], &body,
        );
    }
    // Class value -> class value; built-in classes reject user extension of
    // core protocols and Object methods.
    let mut body = vec![LocalGet(0), RefTestNonNull(HeapType::Concrete(CLOSURE)), If(BlockType::Empty),
        LocalGet(0), Call(b.names["closure-environment"]), LocalSet(1)];
    flag(&mut body, 1, nominal::CORE_CLOSED_BIT);
    body.extend([LocalGet(1), RefTestNonNull(HeapType::Concrete(DESCRIPTOR)), If(BlockType::Result(ValType::I32)),
        LocalGet(1), RefCastNonNull(HeapType::Concrete(DESCRIPTOR)),
        StructGet { struct_type_index: DESCRIPTOR, field_index: 0 },
        I64Const(nominal::CORE_CLOSED_BIT - 1), I64And, I64Const(nominal::FIRST_COUNTED_IDENTITY), I64LtS,
        Else, I32Const(0), End, I32Or]);
    body.extend([If(BlockType::Empty)]);
    nominal::message_error(&mut body, "Core protocols of built-in types cannot be extended outside suss.core");
    body.extend([End, End, LocalGet(0)]);
    b.function_with_locals("builtin-extension-guard", &[VALUE], &[VALUE], &[(1, VALUE)], &body);

    for writing in [false, true] {
        // owner/name/(assigned value); payload/schema/index/fresh follow.
        let payload = if writing { 3 } else { 2 };
        let table = payload + 1;
        let slot = table + 1;
        let fresh = slot + 1;
        let normalized = fresh + 1;
        let mut body = vec![];
        guard(&mut body, 1, STRING);
        body.extend([LocalGet(1), Call(normalize), LocalSet(normalized)]);
        body.extend([
            LocalGet(0),
            RefTestNonNull(HeapType::Concrete(4)),
            If(BlockType::Empty),
            LocalGet(0),
            Call(b.names["closure-property-fields"]),
            LocalSet(payload),
        ]);
        if !writing {
            body.push(LocalGet(1));
            name(&mut body, "prototype");
            body.extend([
                Call(equal),
                If(BlockType::Empty),
                LocalGet(0),
                Call(b.names["owned-constructor-prototype"]),
                Return,
                End,
            ]);
            for (spelling, adapter) in [
                ("call", "closure-call-method"),
                ("apply", "closure-apply-method"),
            ] {
                body.push(LocalGet(1));
                name(&mut body, spelling);
                body.extend([
                    Call(equal),
                    If(BlockType::Empty),
                    LocalGet(0),
                    Call(b.names[adapter]),
                    Return,
                    End,
                ]);
            }
        }
        if writing {
            body.push(LocalGet(1));
            name(&mut body, "prototype");
            body.extend([
                Call(equal),
                If(BlockType::Empty),
                LocalGet(0),
                Call(b.names["closure-environment"]),
                LocalSet(fresh),
                I32Const(0),
            ]);
            for descriptor in [
                nominal::ERROR_GLOBAL,
                range_errors::DESCRIPTOR_GLOBAL,
                owned_prototypes::TYPE_ERROR_GLOBAL,
                owned_prototypes::OBJECT_DESCRIPTOR,
            ] {
                body.extend([LocalGet(fresh), GlobalGet(descriptor), RefEq, I32Or]);
            }
            // Pinned unoptimized portable code is non-strict: assignments to
            // these nonwritable builtin prototype properties return the RHS.
            body.extend([If(BlockType::Empty), LocalGet(2), Return, End, End]);
        }
        reject_host_names(&mut body, equal, INHERITED_NAMES);
        reject_host_names(
            &mut body,
            equal,
            &[
                "prototype",
                "length",
                "name",
                "caller",
                "arguments",
                "call",
                "apply",
                "bind",
            ],
        );
        guard(&mut body, payload, ARGS);
        array(&mut body, payload);
        body.extend([ArrayLen, I32Const(3), I32Ne, If(BlockType::Empty)]);
        nominal::error(&mut body);
        body.push(End);
        array(&mut body, payload);
        body.extend([
            I32Const(1),
            ArrayGet(ARGS),
            LocalSet(table),
            LocalGet(table),
            LocalGet(normalized),
            I32Const(2),
            Call(find),
            LocalSet(slot),
            LocalGet(slot),
            I32Const(0),
            I32GeS,
            If(BlockType::Empty),
        ]);
        array(&mut body, table);
        body.extend([LocalGet(slot), I32Const(1), I32Add]);
        if writing {
            body.extend([LocalGet(2), ArraySet(ARGS), LocalGet(2), Return]);
        } else {
            body.extend([ArrayGet(ARGS), Return]);
        }
        body.push(End);
        if writing {
            array(&mut body, table);
            body.extend([
                ArrayLen,
                LocalTee(slot),
                I32Const(arrays::MAX_LENGTH - 2),
                I32GtU,
                If(BlockType::Empty),
            ]);
            nominal::error(&mut body);
            body.extend([
                End,
                LocalGet(slot),
                I32Const(2),
                I32Add,
                ArrayNewDefault(ARGS),
                LocalSet(fresh),
            ]);
            array(&mut body, fresh);
            body.push(I32Const(0));
            array(&mut body, table);
            body.extend([
                I32Const(0),
                LocalGet(slot),
                ArrayCopy {
                    array_type_index_dst: ARGS,
                    array_type_index_src: ARGS,
                },
            ]);
            array(&mut body, fresh);
            body.extend([LocalGet(slot), LocalGet(normalized), ArraySet(ARGS)]);
            array(&mut body, fresh);
            body.extend([
                LocalGet(slot),
                I32Const(1),
                I32Add,
                LocalGet(2),
                ArraySet(ARGS),
            ]);
            array(&mut body, payload);
            body.extend([
                I32Const(1),
                LocalGet(fresh),
                ArraySet(ARGS),
                LocalGet(2),
                Return,
            ]);
        } else {
            undefined(&mut body);
            body.push(Return);
        }
        body.push(End);

        // Strings and source arrays provide their actual UTF-16/indexed length.
        body.extend([
            LocalGet(0),
            RefTestNonNull(HeapType::Concrete(STRING)),
            If(BlockType::Empty),
        ]);
        if writing {
            nominal::error(&mut body);
        } else {
            body.push(LocalGet(1));
            name(&mut body, "length");
            body.extend([
                Call(equal),
                If(BlockType::Empty),
                LocalGet(0),
                ArrayNewFixed {
                    array_type_index: ARGS,
                    array_size: 1,
                },
                Call(b.names["source-array-length-args"]),
                Return,
                End,
            ]);
            body.push(LocalGet(1));
            name(&mut body, "charCodeAt");
            body.extend([
                Call(equal),
                If(BlockType::Empty),
                Call(b.names["string-char-code-at-method"]),
                Return,
                End,
            ]);
            body.push(LocalGet(1));
            name(&mut body, "charAt");
            body.extend([
                Call(equal),
                If(BlockType::Empty),
                Call(b.names["string-char-at-method"]),
                Return,
                End,
            ]);
            reject_host_names(&mut body, equal, INHERITED_NAMES);
            undefined(&mut body);
            body.push(Return);
        }
        body.push(End);
        body.extend([
            LocalGet(0),
            RefTestNonNull(HeapType::Concrete(7)),
            If(BlockType::Empty),
        ]);
        structure(&mut body, 0, 7, 0);
        body.extend([GlobalGet(arrays::TAG_GLOBAL), RefEq, If(BlockType::Empty)]);
        if writing {
            body.extend([
                LocalGet(0),
                LocalGet(1),
                LocalGet(2),
                Call(b.names["source-array-set"]),
                Return,
            ]);
        } else {
            body.extend([
                LocalGet(0),
                LocalGet(1),
                Call(b.names["source-array-property-has"]),
                If(BlockType::Empty),
                LocalGet(0),
                LocalGet(1),
                Call(b.names["source-array-property-get"]),
                Return,
                End,
            ]);
            body.push(LocalGet(1));
            name(&mut body, "length");
            body.extend([
                Call(equal),
                If(BlockType::Empty),
                LocalGet(0),
                ArrayNewFixed {
                    array_type_index: ARGS,
                    array_size: 1,
                },
                Call(b.names["source-array-length-args"]),
                Return,
                End,
            ]);
            body.push(LocalGet(1));
            name(&mut body, "pop");
            body.extend([
                Call(equal),
                If(BlockType::Empty),
                Call(b.names["source-array-pop-method"]),
                Return,
                End,
            ]);
            body.push(LocalGet(1));
            name(&mut body, "push");
            body.extend([
                Call(equal),
                If(BlockType::Empty),
                Call(b.names["source-array-push-method"]),
                Return,
                End,
            ]);
            body.push(LocalGet(1));
            name(&mut body, "shift");
            body.extend([
                Call(equal),
                If(BlockType::Empty),
                Call(b.names["source-array-shift-method"]),
                Return,
                End,
            ]);
            body.push(LocalGet(1));
            name(&mut body, "slice");
            body.extend([
                Call(equal),
                If(BlockType::Empty),
                Call(b.names["source-array-slice-method"]),
                Return,
                End,
            ]);
            body.push(LocalGet(1));
            name(&mut body, "hasOwnProperty");
            body.extend([
                Call(equal),
                If(BlockType::Empty),
                Call(b.names["source-array-has-own-method"]),
                Return,
                End,
            ]);
            for (name_value, export) in [
                ("join", "source-array-join-method"),
                ("valueOf", "source-array-valueOf-method"),
                ("toString", "source-array-toString-method"),
            ] {
                body.push(LocalGet(1));
                name(&mut body, name_value);
                body.extend([
                    Call(equal),
                    If(BlockType::Empty),
                    Call(b.names[export]),
                    Return,
                    End,
                ]);
            }
            body.extend([
                LocalGet(0),
                LocalGet(1),
                Call(b.names["source-array-get"]),
                Return,
            ]);
        }
        body.push(End);
        structure(&mut body, 0, 7, 1);
        body.push(LocalSet(payload));
        structure(&mut body, 0, 7, 0);
        body.extend([
            StructGet {
                struct_type_index: DESCRIPTOR,
                field_index: 1,
            },
            LocalSet(table),
        ]);
        guard(&mut body, payload, ARGS);
        guard(&mut body, table, ARGS);
        array(&mut body, payload);
        body.push(ArrayLen);
        array(&mut body, table);
        body.extend([ArrayLen, I32Ne, If(BlockType::Empty)]);
        nominal::error(&mut body);
        body.extend([
            End,
            LocalGet(table),
            LocalGet(1),
            I32Const(1),
            Call(find),
            LocalSet(slot),
            LocalGet(slot),
            I32Const(0),
            I32GeS,
            If(BlockType::Empty),
        ]);
        array(&mut body, payload);
        body.push(LocalGet(slot));
        if writing {
            body.extend([LocalGet(2), ArraySet(ARGS), LocalGet(2), Return]);
        } else {
            body.extend([ArrayGet(ARGS), Return]);
        }
        body.push(End);
        if writing {
            nominal::error(&mut body); // Dynamic extra instance fields are not implemented.
        } else {
            structure(&mut body, 0, 7, 0);
            body.extend([
                LocalGet(1),
                Call(b.names["object-method-key"]),
                LocalSet(fresh),
                LocalGet(fresh),
                I32Const(0),
                RefI31,
                RefEq,
                I32Eqz,
                If(BlockType::Empty),
            ]);
            structure(&mut body, 0, 7, 0);
            body.extend([
                LocalGet(fresh),
                Call(b.names["protocol-method-get"]),
                LocalSet(fresh),
                LocalGet(fresh),
                I32Const(0),
                RefI31,
                RefEq,
                I32Eqz,
                If(BlockType::Empty),
                LocalGet(fresh),
                Return,
                End,
                End,
            ]);
            body.extend([
                LocalGet(0),
                LocalGet(1),
                Call(b.names["owned-instance-property-get"]),
                LocalSet(fresh),
                LocalGet(fresh),
                RefTestNonNull(HeapType::Concrete(ARGS)),
                If(BlockType::Empty),
                LocalGet(fresh),
                RefCastNonNull(HeapType::Concrete(ARGS)),
                I32Const(0),
                ArrayGet(ARGS),
                Return,
                End,
            ]);
            reject_host_names(&mut body, equal, INHERITED_NAMES);
            undefined(&mut body);
            body.push(Return);
        }
        body.push(End);
        // Nil and undefined throw; other unsupported owner shapes stay explicit.
        nominal::error(&mut body);
        b.function_with_locals(
            if writing {
                "fixed-named-property-set"
            } else {
                "fixed-named-property-get"
            },
            if writing {
                &[VALUE, VALUE, VALUE]
            } else {
                &[VALUE, VALUE]
            },
            &[VALUE],
            &[(2, VALUE), (1, ValType::I32), (2, VALUE)],
            &body,
        );
    }
    callbacks.extend(callable_properties::functions(b, equal));
    callbacks
}
