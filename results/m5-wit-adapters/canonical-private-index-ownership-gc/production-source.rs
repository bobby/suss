//! Original owned canonical memory allocator. Each block has a private header;
//! freeing retains its capacity on a separate free list. Reallocation copies
//! before freeing. A private GC bitmap validates live allocation starts without
//! trusting caller-writable headers or scanning every retained block.
//! Source values must copy bytes into GC storage before releasing these blocks.
use wasm_encoder::*;

const HEADER: u64 = 16;
fn access(offset: u64) -> MemArg {
    MemArg {
        offset,
        align: 2,
        memory_index: 0,
    }
}

/// A separate memory instance lets canonical lowering allocate before the source
/// adapter is instantiated. No borrowed linear-memory pointer enters a GC value.
pub(crate) fn module() -> Vec<u8> {
    use Instruction::*;
    let mut types = TypeSection::new();
    types
        .ty()
        .function([ValType::I32, ValType::I32], [ValType::I32]); // allocate
    types.ty().function([ValType::I32, ValType::I32], []); // release
    types.ty().function([ValType::I32; 4], [ValType::I32]); // realloc
    types.ty().function([ValType::I32], [ValType::I32]); // locate
    types
        .ty()
        .function([ValType::I32, ValType::I32], [ValType::I32]); // check
    types.ty().array(&StorageType::I8, true); // 5: private ownership bitmap
    let mut functions = FunctionSection::new();
    for ty in [0, 1, 2, 3, 4, 1] {
        functions.function(ty);
    }
    let mut memories = MemorySection::new();
    memories.memory(MemoryType {
        minimum: 1,
        maximum: Some(65536),
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    let mut globals = GlobalSection::new();
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &ConstExpr::i32_const(0),
    ); // head pointer
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &ConstExpr::i32_const(16),
    ); // end
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &ConstExpr::i32_const(0),
    ); // free-list head, linked by inactive headers' length field
    globals.global(
        GlobalType {
            val_type: ValType::Ref(RefType {
                nullable: true,
                heap_type: HeapType::Concrete(5),
            }),
            mutable: true,
            shared: false,
        },
        &ConstExpr::ref_null(HeapType::Concrete(5)),
    ); // private active-start bitmap: one bit per 16 bytes of canonical memory
    let mut exports = ExportSection::new();
    exports.export("memory", ExportKind::Memory, 0);
    exports.export("cabi_realloc", ExportKind::Func, 2);
    exports.export("release", ExportKind::Func, 1);
    let mut code = CodeSection::new();

    // Parameters size/align; locals pointer/header (i32), pointer/end/pages (i64).
    let mut allocate = Function::new([(2, ValType::I32), (3, ValType::I64), (1, ValType::I32)]);
    emit(
        &mut allocate,
        &[
            // All supported canonical alignments are powers of two, bounded by a page.
            LocalGet(1),
            I32Eqz,
            LocalGet(1),
            I32Const(65536),
            I32GtU,
            I32Or,
            LocalGet(1),
            LocalGet(1),
            I32Const(1),
            I32Sub,
            I32And,
            I32Eqz,
            I32Eqz,
            I32Or,
            If(BlockType::Empty),
            Unreachable,
            End,
            LocalGet(0),
            I32Eqz,
            If(BlockType::Empty),
            I32Const(0),
            Return,
            End,
            GlobalGet(2),
            LocalSet(2),
            I32Const(0),
            LocalSet(7),
            Block(BlockType::Empty),
            Loop(BlockType::Empty),
            LocalGet(2),
            I32Eqz,
            BrIf(1),
            LocalGet(2),
            I32Const(HEADER as i32),
            I32Sub,
            LocalSet(3),
            LocalGet(3),
            I32Load(access(12)),
            I32Eqz,
            LocalGet(3),
            I32Load(access(4)),
            LocalGet(0),
            I32GeU,
            I32And,
            LocalGet(2),
            LocalGet(1),
            I32Const(1),
            I32Sub,
            I32And,
            I32Eqz,
            I32And,
            If(BlockType::Empty),
            // Unlink this free block before publishing its new live length.
            LocalGet(7),
            I32Eqz,
            If(BlockType::Empty),
            LocalGet(3),
            I32Load(access(8)),
            GlobalSet(2),
            Else,
            LocalGet(7),
            I32Const(HEADER as i32),
            I32Sub,
            LocalGet(3),
            I32Load(access(8)),
            I32Store(access(8)),
            End,
            LocalGet(2),
            I32Const(1),
            Call(5),
            LocalGet(3),
            I32Const(1),
            I32Store(access(12)),
            LocalGet(3),
            LocalGet(0),
            I32Store(access(8)),
            LocalGet(2),
            Return,
            End,
            LocalGet(2),
            LocalSet(7),
            LocalGet(3),
            I32Load(access(8)),
            LocalSet(2),
            Br(0),
            End,
            End,
            // Do arithmetic in i64; neither alignment nor size may wrap memory32.
            GlobalGet(1),
            I64ExtendI32U,
            I64Const(HEADER as i64),
            I64Add,
            LocalGet(1),
            I64ExtendI32U,
            I64Const(1),
            I64Sub,
            I64Add,
            LocalGet(1),
            I64ExtendI32U,
            I64Const(1),
            I64Sub,
            I64Const(-1),
            I64Xor,
            I64And,
            LocalSet(4),
            LocalGet(4),
            LocalGet(0),
            I64ExtendI32U,
            I64Add,
            I64Const(15),
            I64Add,
            I64Const(-16),
            I64And,
            LocalTee(5),
            I64Const(u32::MAX as i64),
            I64GtU,
            If(BlockType::Empty),
            Unreachable,
            End,
            LocalGet(5),
            I64Const(65535),
            I64Add,
            I64Const(16),
            I64ShrU,
            LocalSet(6),
            LocalGet(6),
            MemorySize(0),
            I64ExtendI32U,
            I64GtU,
            If(BlockType::Empty),
            LocalGet(6),
            I32WrapI64,
            MemorySize(0),
            I32Sub,
            MemoryGrow(0),
            I32Const(-1),
            I32Eq,
            If(BlockType::Empty),
            Unreachable,
            End,
            End,
            LocalGet(4),
            I32WrapI64,
            LocalSet(2),
            LocalGet(2),
            I32Const(1),
            Call(5),
            LocalGet(2),
            I32Const(HEADER as i32),
            I32Sub,
            LocalSet(3),
            LocalGet(3),
            GlobalGet(0),
            I32Store(access(0)),
            LocalGet(3),
            LocalGet(0),
            I32Store(access(4)),
            LocalGet(3),
            LocalGet(0),
            I32Store(access(8)),
            LocalGet(3),
            I32Const(1),
            I32Store(access(12)),
            LocalGet(2),
            GlobalSet(0),
            LocalGet(5),
            I32WrapI64,
            GlobalSet(1),
            LocalGet(2),
            End,
        ],
    );
    code.function(&allocate);

    let mut release = Function::new([(1, ValType::I32)]);
    emit(
        &mut release,
        &[
            LocalGet(0),
            I32Eqz,
            If(BlockType::Empty),
            LocalGet(1),
            If(BlockType::Empty),
            Unreachable,
            End,
            Return,
            End,
            LocalGet(0),
            LocalGet(1),
            Call(4),
            LocalSet(2),
            LocalGet(0),
            I32Const(0),
            Call(5),
            LocalGet(2),
            I32Const(0),
            I32Store(access(12)),
            LocalGet(2),
            GlobalGet(2),
            I32Store(access(8)),
            LocalGet(0),
            GlobalSet(2),
            End,
        ],
    );
    code.function(&release);

    // Parameters oldptr/oldsize/align/newsize; locals header/newptr.
    let mut realloc = Function::new([(2, ValType::I32)]);
    emit(
        &mut realloc,
        &[
            LocalGet(2),
            I32Eqz,
            LocalGet(2),
            I32Const(65536),
            I32GtU,
            I32Or,
            LocalGet(2),
            LocalGet(2),
            I32Const(1),
            I32Sub,
            I32And,
            I32Eqz,
            I32Eqz,
            I32Or,
            If(BlockType::Empty),
            Unreachable,
            End,
            LocalGet(0),
            I32Eqz,
            If(BlockType::Empty),
            LocalGet(1),
            If(BlockType::Empty),
            Unreachable,
            End,
            LocalGet(3),
            LocalGet(2),
            Call(0),
            Return,
            End,
            LocalGet(0),
            LocalGet(1),
            Call(4),
            LocalSet(4),
            LocalGet(3),
            I32Eqz,
            If(BlockType::Empty),
            LocalGet(0),
            LocalGet(1),
            Call(1),
            I32Const(0),
            Return,
            End,
            LocalGet(4),
            I32Load(access(4)),
            LocalGet(3),
            I32GeU,
            LocalGet(0),
            LocalGet(2),
            I32Const(1),
            I32Sub,
            I32And,
            I32Eqz,
            I32And,
            If(BlockType::Empty),
            LocalGet(4),
            LocalGet(3),
            I32Store(access(8)),
            LocalGet(0),
            Return,
            End,
            LocalGet(3),
            LocalGet(2),
            Call(0),
            LocalSet(5),
            LocalGet(5),
            LocalGet(0),
            LocalGet(1),
            LocalGet(3),
            LocalGet(1),
            LocalGet(3),
            I32LtU,
            Select,
            MemoryCopy {
                src_mem: 0,
                dst_mem: 0,
            },
            LocalGet(0),
            LocalGet(1),
            Call(1),
            LocalGet(5),
            End,
        ],
    );
    code.function(&realloc);

    // Active ownership lives outside caller-writable linear memory. Every
    // allocated payload starts at a 16-byte boundary, even for alignments 1/2/4.
    // Membership is checked before reading a purported private header.
    let mut locate = Function::new([(1, ValType::I32)]);
    emit(
        &mut locate,
        &[
            LocalGet(0),
            I32Const(15),
            I32And,
            If(BlockType::Empty),
            Unreachable,
            End,
            GlobalGet(3),
            RefIsNull,
            If(BlockType::Empty),
            Unreachable,
            End,
            LocalGet(0),
            I32Const(7),
            I32ShrU,
            LocalSet(1),
            LocalGet(1),
            GlobalGet(3),
            RefAsNonNull,
            ArrayLen,
            I32GeU,
            If(BlockType::Empty),
            Unreachable,
            End,
            GlobalGet(3),
            RefAsNonNull,
            LocalGet(1),
            ArrayGetU(5),
            I32Const(1),
            LocalGet(0),
            I32Const(4),
            I32ShrU,
            I32Const(7),
            I32And,
            I32Shl,
            I32And,
            I32Eqz,
            If(BlockType::Empty),
            Unreachable,
            End,
            LocalGet(0),
            I32Const(HEADER as i32),
            I32Sub,
            End,
        ],
    );
    code.function(&locate);
    let mut check = Function::new([(1, ValType::I32)]);
    emit(
        &mut check,
        &[
            LocalGet(0),
            Call(3),
            LocalSet(2),
            LocalGet(2),
            I32Load(access(12)),
            I32Const(1),
            I32Ne,
            LocalGet(2),
            I32Load(access(8)),
            LocalGet(1),
            I32Ne,
            I32Or,
            If(BlockType::Empty),
            Unreachable,
            End,
            LocalGet(2),
            End,
        ],
    );
    code.function(&check);

    // mark(pointer, active): bitmap capacity follows checked canonical-memory
    // growth. The largest memory32 needs 32 MiB of private packed bitmap, and
    // growth copies the previous active-start bits before publishing an owner.
    let mut mark = Function::new([
        (3, ValType::I32),
        (
            1,
            ValType::Ref(RefType {
                nullable: true,
                heap_type: HeapType::Concrete(5),
            }),
        ),
    ]);
    emit(
        &mut mark,
        &[
            LocalGet(0),
            I32Const(7),
            I32ShrU,
            LocalSet(2),
            I32Const(1),
            LocalGet(0),
            I32Const(4),
            I32ShrU,
            I32Const(7),
            I32And,
            I32Shl,
            LocalSet(4),
            GlobalGet(3),
            RefIsNull,
            If(BlockType::Result(ValType::I32)),
            I32Const(1),
            Else,
            LocalGet(2),
            GlobalGet(3),
            RefAsNonNull,
            ArrayLen,
            I32GeU,
            End,
            If(BlockType::Empty),
            GlobalGet(3),
            LocalSet(5),
            MemorySize(0),
            I32Const(9),
            I32Shl,
            ArrayNewDefault(5),
            GlobalSet(3),
            LocalGet(5),
            RefIsNull,
            I32Eqz,
            If(BlockType::Empty),
            GlobalGet(3),
            RefAsNonNull,
            I32Const(0),
            LocalGet(5),
            RefAsNonNull,
            I32Const(0),
            LocalGet(5),
            RefAsNonNull,
            ArrayLen,
            ArrayCopy {
                array_type_index_dst: 5,
                array_type_index_src: 5,
            },
            End,
            End,
            GlobalGet(3),
            RefAsNonNull,
            LocalGet(2),
            ArrayGetU(5),
            LocalSet(3),
            // Internal transitions are exclusive: allocation must acquire a free
            // start and release must clear a live one.
            LocalGet(3),
            LocalGet(4),
            I32And,
            I32Eqz,
            LocalGet(1),
            I32Ne,
            If(BlockType::Empty),
            Unreachable,
            End,
            GlobalGet(3),
            RefAsNonNull,
            LocalGet(2),
            LocalGet(1),
            If(BlockType::Result(ValType::I32)),
            LocalGet(3),
            LocalGet(4),
            I32Or,
            Else,
            LocalGet(3),
            LocalGet(4),
            I32Const(-1),
            I32Xor,
            I32And,
            End,
            ArraySet(5),
            End,
        ],
    );
    code.function(&mark);
    let mut module = Module::new();
    module
        .section(&types)
        .section(&functions)
        .section(&memories)
        .section(&globals)
        .section(&exports)
        .section(&code);
    module.finish()
}

fn emit(function: &mut Function, instructions: &[Instruction<'_>]) {
    for instruction in instructions {
        function.instruction(instruction);
    }
}

#[cfg(all(test, not(suss_generator)))]
mod tests {
    use super::module;
    use std::sync::OnceLock;
    use wasmtime::{
        Engine, Instance, Memory, Module, Store, StoreLimits, StoreLimitsBuilder, TypedFunc,
    };

    type Realloc = TypedFunc<(i32, i32, i32, i32), i32>;
    type Release = TypedFunc<(i32, i32), ()>;
    fn allocator(limit: usize) -> (Store<StoreLimits>, Memory, Realloc, Release) {
        static ENGINE: OnceLock<Engine> = OnceLock::new();
        let engine = ENGINE.get_or_init(|| {
            let mut config = wasmtime::Config::new();
            config.wasm_gc(true).consume_fuel(true);
            Engine::new(&config).unwrap()
        });
        let module = Module::new(engine, module()).unwrap();
        let mut store = Store::new(engine, StoreLimitsBuilder::new().memory_size(limit).build());
        store.limiter(|limits| limits);
        store.set_fuel(100_000_000).unwrap();
        let instance = Instance::new(&mut store, &module, &[]).unwrap();
        let memory = instance.get_memory(&mut store, "memory").unwrap();
        let realloc = instance.get_typed_func(&mut store, "cabi_realloc").unwrap();
        let release = instance.get_typed_func(&mut store, "release").unwrap();
        (store, memory, realloc, release)
    }

    #[test]
    fn ten_thousand_live_varied_blocks_allocate_release_and_reuse_with_bounded_fuel() {
        let (mut store, memory, realloc, release) = allocator(64 * 65536);
        let mut blocks = Vec::new();
        for index in 0..10_000usize {
            let size = (index % 63 + 1) as i32;
            let alignment = 1 << (index % 4);
            let pointer = realloc
                .call(&mut store, (0, 0, alignment, size))
                .unwrap_or_else(|error| panic!("allocate live block {index}: {error:?}"));
            assert_eq!(pointer % alignment, 0);
            memory
                .write(&mut store, pointer as usize, &[(index % 251) as u8])
                .unwrap();
            blocks.push((pointer, size, alignment));
            if index % 1024 == 0 {
                store.gc(None).unwrap();
            }
        }
        let pages = memory.size(&store);
        for (index, &(pointer, size, _)) in blocks.iter().enumerate() {
            let mut value = [0];
            memory.read(&store, pointer as usize, &mut value).unwrap();
            assert_eq!(value[0], (index % 251) as u8);
            release
                .call(&mut store, (pointer, size))
                .unwrap_or_else(|error| panic!("release live block {index}: {error:?}"));
        }
        let mut reused = Vec::new();
        for &(pointer, size, alignment) in blocks.iter().rev() {
            let again = realloc.call(&mut store, (0, 0, alignment, size)).unwrap();
            assert_eq!(again, pointer, "reuse the matching freed capacity");
            reused.push((again, size));
        }
        for (pointer, size) in reused {
            release.call(&mut store, (pointer, size)).unwrap();
        }
        assert_eq!(memory.size(&store), pages);
        println!(
            "10000 varied live blocks, release and reuse: {} fuel consumed",
            100_000_000 - store.get_fuel().unwrap()
        );
    }

    #[test]
    fn private_live_index_rejects_forged_headers_and_survives_gc_and_page_alignment() {
        let (mut store, memory, realloc, release) = allocator(4 * 65536);
        let first = realloc.call(&mut store, (0, 0, 16, 128)).unwrap();
        let other = realloc.call(&mut store, (0, 0, 65536, 16)).unwrap();
        assert_eq!(other % 65536, 0);
        let fake = first + 32; // aligned interior pointer with a plausible header
        let mut header = [0u8; 16];
        header[4..8].copy_from_slice(&16u32.to_le_bytes());
        header[8..12].copy_from_slice(&16u32.to_le_bytes());
        header[12..16].copy_from_slice(&1u32.to_le_bytes());
        memory
            .write(&mut store, (fake - 16) as usize, &header)
            .unwrap();
        store.gc(None).unwrap();
        assert!(release.call(&mut store, (fake, 16)).is_err());
        assert!(realloc.call(&mut store, (fake, 16, 16, 32)).is_err());
        release.call(&mut store, (first, 128)).unwrap();
        // Caller-writable linear metadata cannot resurrect a released owner.
        memory
            .write(&mut store, (first - 4) as usize, &1u32.to_le_bytes())
            .unwrap();
        assert!(release.call(&mut store, (first, 128)).is_err());
        memory
            .write(&mut store, (first - 4) as usize, &0u32.to_le_bytes())
            .unwrap();
        let again = realloc.call(&mut store, (0, 0, 16, 96)).unwrap();
        assert_eq!(again, first);
        assert!(release.call(&mut store, (again, 128)).is_err());
        store.gc(None).unwrap();
        release.call(&mut store, (again, 96)).unwrap();
        release.call(&mut store, (other, 16)).unwrap();
    }

    #[test]
    fn canonical_realloc_copies_grows_shrinks_and_reuses_freed_capacity() {
        let (mut store, memory, realloc, release) = allocator(4 * 65536);
        let first = realloc.call(&mut store, (0, 0, 8, 32)).unwrap();
        assert_eq!(first % 8, 0);
        let pattern = (0..32).map(|x| x as u8).collect::<Vec<_>>();
        memory.write(&mut store, first as usize, &pattern).unwrap();
        let grown = realloc.call(&mut store, (first, 32, 16, 70000)).unwrap();
        assert_eq!(grown % 16, 0);
        assert_ne!(grown, first);
        assert_eq!(memory.size(&store), 2);
        let mut bytes = [0; 32];
        memory.read(&store, grown as usize, &mut bytes).unwrap();
        assert_eq!(bytes.as_slice(), pattern);
        let reused = realloc.call(&mut store, (0, 0, 8, 24)).unwrap();
        assert_eq!(reused, first, "moving realloc releases the original block");
        let shrunk = realloc.call(&mut store, (grown, 70000, 16, 8)).unwrap();
        assert_eq!(shrunk, grown);
        let expanded = realloc.call(&mut store, (shrunk, 8, 16, 60000)).unwrap();
        assert_eq!(expanded, grown, "shrinking retains reusable capacity");
        memory.read(&store, expanded as usize, &mut bytes).unwrap();
        assert_eq!(bytes.as_slice(), pattern);
        assert_eq!(
            realloc.call(&mut store, (expanded, 60000, 16, 0)).unwrap(),
            0
        );
        for _ in 0..1000 {
            let pointer = realloc.call(&mut store, (0, 0, 16, 60000)).unwrap();
            assert_eq!(pointer, grown);
            release.call(&mut store, (pointer, 60000)).unwrap();
        }
        assert_eq!(
            memory.size(&store),
            2,
            "repeated ownership cycles must not grow memory"
        );
        release.call(&mut store, (reused, 24)).unwrap();
        assert_eq!(realloc.call(&mut store, (0, 0, 1, 0)).unwrap(), 0);
        release.call(&mut store, (0, 0)).unwrap();
    }

    #[test]
    fn canonical_allocator_rejects_invalid_ownership_alignment_and_overflow() {
        let (mut store, _, realloc, release) = allocator(2 * 65536);
        let pointer = realloc.call(&mut store, (0, 0, 8, 32)).unwrap();
        for alignment in [0, 3, 65537, -1] {
            assert!(realloc.call(&mut store, (0, 0, alignment, 8)).is_err());
        }
        assert!(realloc.call(&mut store, (0, 0, 8, -1)).is_err());
        assert!(realloc.call(&mut store, (0, 1, 8, 16)).is_err());
        assert!(realloc.call(&mut store, (pointer + 8, 32, 8, 64)).is_err());
        assert!(realloc.call(&mut store, (pointer, 31, 8, 64)).is_err());
        assert!(release.call(&mut store, (pointer, 31)).is_err());
        assert!(release.call(&mut store, (0, 1)).is_err());
        release.call(&mut store, (pointer, 32)).unwrap();
        assert!(release.call(&mut store, (pointer, 32)).is_err());
        assert_eq!(realloc.call(&mut store, (0, 0, 8, 32)).unwrap(), pointer);
    }

    #[test]
    fn failed_growth_preserves_original_owned_block_and_contents() {
        let (mut store, memory, realloc, release) = allocator(2 * 65536);
        let pointer = realloc.call(&mut store, (0, 0, 1, 16)).unwrap();
        memory
            .write(&mut store, pointer as usize, b"retained payload")
            .unwrap();
        assert!(realloc.call(&mut store, (pointer, 16, 8, 200000)).is_err());
        assert_eq!(memory.size(&store), 1);
        let mut bytes = [0; 16];
        memory.read(&store, pointer as usize, &mut bytes).unwrap();
        assert_eq!(&bytes, b"retained payload");
        release.call(&mut store, (pointer, 16)).unwrap();
        assert_eq!(realloc.call(&mut store, (0, 0, 1, 16)).unwrap(), pointer);
    }
}
