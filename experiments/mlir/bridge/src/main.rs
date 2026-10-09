//! Isolated bounded MLIR transport and genuine source-facts preservation probes.
//! Runtime graphs are hand-authored; shipped compilation paths are unchanged.
mod abi_probe;
mod analysis;
mod effects;
mod source_section;
mod comparison;
use serde::Deserialize;
use std::{error::Error, io::Read};
use suss_compile::{
    portable::{
        self,
        hir::{Arithmetic, Literal, Type},
        ir::{self, ValueId},
        resolve::{Environment, Phase},
    },
    runtime_abi,
};
use wasmtime::{
    Config, Engine, Global, GlobalType, Instance, Linker, Module, Mutability, RefType, Store, Val,
    ValType,
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
fn require(ok: bool, message: &str) -> Result<()> {
    if ok { Ok(()) } else { Err(message.into()) }
}
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
// Empty struct variants enforce unknown-field rejection; tagged unit variants did not.
enum Ty {
    #[serde(rename = "f64")]
    F64 {},
    #[serde(rename = "closure")]
    Closure { arity: usize, captures: Vec<Ty> },
}
impl Ty {
    fn portable(&self) -> Type {
        match self {
            Self::F64 {} => Type::Number,
            Self::Closure { arity, .. } => Type::Closure(*arity),
        }
    }
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Graph {
    parameters: Vec<Ty>,
    operations: Vec<Op>,
    return_value: usize,
}
#[derive(Clone, Deserialize)]
#[serde(tag = "op", deny_unknown_fields)]
enum Op {
    #[serde(rename = "suss.const")]
    Const { bits: String },
    #[serde(rename = "suss.add")]
    Add { lhs: usize, rhs: usize },
    #[serde(rename = "suss.closure")]
    Closure {
        captures: Vec<usize>,
        arity: usize,
        body: Graph,
    },
    #[serde(rename = "suss.call")]
    Call {
        callee: usize,
        arguments: Vec<usize>,
    },
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    schema: String,
    verified_mlir: bool,
    producer: Graph,
    caller: Graph,
    expected_bits: String,
    #[serde(default, deserialize_with = "present_source_analysis")]
    source_analysis: Option<String>,
}
fn present_source_analysis<'de, D>(deserializer: D) -> std::result::Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    // Missing is v1; present must be a string. Explicit null must not silently
    // become None and extend the closed v1 profile.
    String::deserialize(deserializer).map(Some)
}
fn bits(s: &str) -> Result<u64> {
    require(
        s.len() == 16
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "bits must be exactly 16 lowercase hex digits",
    )?;
    Ok(u64::from_str_radix(s, 16)?)
}
// IDs are local: entry parameters first, then exactly one result per operation.
fn reconstruct(
    g: &Graph,
    global: Option<&portable::resolve::Global>,
    depth: usize,
    budget: &mut usize,
) -> Result<(ir::Function, Ty)> {
    require(
        depth <= 16 && g.parameters.len() <= 32 && g.operations.len() <= 4096,
        "graph limit exceeded",
    )?;
    *budget = budget
        .checked_sub(g.operations.len() + g.parameters.len())
        .ok_or("total graph limit exceeded")?;
    let mut types = g.parameters.clone();
    let mut values: Vec<_> = types
        .iter()
        .map(|ty| ir::Value {
            ty: ty.portable(),
            span: 0..0,
        })
        .collect();
    let mut instructions = Vec::new();
    let parameters = if let Some(id) = global {
        require(
            g.parameters.len() == 1 && matches!(g.parameters[0], Ty::Closure { .. }),
            "caller requires one closure parameter",
        )?;
        // Explicit transport linkage: parameter 0 denotes the producer's published cell.
        values[0].ty = Type::Value;
        instructions.push(ir::Instruction {
            result: ValueId(0),
            operation: ir::Operation::GlobalRead(id.clone()),
            span: 0..0,
        });
        vec![]
    } else {
        (0..g.parameters.len()).map(ValueId).collect()
    };
    for op in &g.operations {
        let get = |id: usize| {
            types
                .get(id)
                .cloned()
                .ok_or_else(|| -> Box<dyn Error> { "forward/out-of-range SSA use".into() })
        };
        let (operation, ty, portable_ty) = match op {
            Op::Const { bits: raw } => (
                ir::Operation::Literal(Literal::Number(f64::from_bits(bits(raw)?))),
                Ty::F64 {},
                Type::Number,
            ),
            Op::Add { lhs, rhs } => {
                require(
                    get(*lhs)? == (Ty::F64 {}) && get(*rhs)? == (Ty::F64 {}),
                    "add requires f64",
                )?;
                let ty = if values[*lhs].ty == Type::Number && values[*rhs].ty == Type::Number {
                    Type::Number
                } else {
                    Type::Value
                };
                (
                    ir::Operation::Arithmetic {
                        operator: Arithmetic::Add,
                        arguments: vec![ValueId(*lhs), ValueId(*rhs)],
                    },
                    Ty::F64 {},
                    ty,
                )
            }
            Op::Closure {
                captures,
                arity,
                body,
            } => {
                require(
                    depth == 0,
                    "nested closure construction unsupported in this draft",
                )?;
                require(*arity <= 32, "arity limit exceeded")?;
                let capture_types = captures
                    .iter()
                    .map(|id| get(*id))
                    .collect::<Result<Vec<_>>>()?;
                // Bound this draft to numeric captures and fixed all-f64 signatures.
                require(
                    capture_types.iter().all(|ty| *ty == (Ty::F64 {})),
                    "only f64 captures supported",
                )?;
                let mut entry = capture_types.clone();
                entry.extend(std::iter::repeat_n(Ty::F64 {}, *arity));
                require(
                    body.parameters == entry,
                    "closure entry must be captures then fixed f64 arguments",
                )?;
                let (mut inner, result) = reconstruct(body, None, depth + 1, budget)?;
                require(result == (Ty::F64 {}), "closure must return f64")?;
                // Universal ABI arguments are dynamic Values; capture types retain producer IR types.
                let ct: Vec<_> = captures.iter().map(|id| values[*id].ty).collect();
                for (i, ty) in ct
                    .iter()
                    .copied()
                    .chain(std::iter::repeat_n(Type::Value, *arity))
                    .enumerate()
                {
                    inner.values[i].ty = ty;
                }
                // Recompute add result typing in source order after entry ABI adaptation.
                for inst in &inner.blocks[0].instructions {
                    if let ir::Operation::Arithmetic { arguments, .. } = &inst.operation {
                        inner.values[inst.result.0].ty = if arguments
                            .iter()
                            .all(|id| inner.values[id.0].ty == Type::Number)
                        {
                            Type::Number
                        } else {
                            Type::Value
                        };
                    }
                }
                (
                    ir::Operation::MakeClosure {
                        body: Box::new(ir::ClosureBody {
                            capture_types: ct,
                            arity: *arity,
                            variadic: false,
                            display_name: None,
                            function: inner,
                        }),
                        captures: captures.iter().copied().map(ValueId).collect(),
                    },
                    Ty::Closure {
                        arity: *arity,
                        captures: capture_types,
                    },
                    Type::Closure(*arity),
                )
            }
            Op::Call { callee, arguments } => {
                let Ty::Closure { arity, .. } = get(*callee)? else {
                    return Err("call requires closure".into());
                };
                require(arguments.len() == arity, "call arity mismatch")?;
                for id in arguments {
                    require(get(*id)? == (Ty::F64 {}), "call arguments must be f64")?;
                }
                (
                    ir::Operation::Call {
                        operands: std::iter::once(*callee)
                            .chain(arguments.iter().copied())
                            .map(ValueId)
                            .collect(),
                    },
                    Ty::F64 {},
                    Type::Value,
                )
            }
        };
        let result = ValueId(types.len());
        types.push(ty);
        values.push(ir::Value {
            ty: portable_ty,
            span: 0..0,
        });
        instructions.push(ir::Instruction {
            result,
            operation,
            span: 0..0,
        });
    }
    let result = types
        .get(g.return_value)
        .cloned()
        .ok_or("return out of range")?;
    Ok((
        ir::Function {
            values,
            blocks: vec![ir::Block {
                parameters,
                instructions,
                terminator: ir::Terminator::Return(ValueId(g.return_value)),
            }],
            span: 0..0,
        },
        result,
    ))
}
fn call(store: &mut Store<()>, instance: Instance, name: &str, args: &[Val]) -> Result<Val> {
    let mut out = [Val::null_any_ref()];
    instance
        .get_func(&mut *store, name)
        .ok_or("missing function")?
        .call(store, args, &mut out)?;
    Ok(out[0].clone())
}
fn fragment(
    store: &mut Store<()>,
    linker: &Linker<()>,
    f: &ir::Function,
    source: Option<&str>,
) -> Result<Instance> {
    ir::verify(f)?;
    let mut wasm = portable::compile_ir(f)?;
    if let Some(source) = source {
        source_section::append(&mut wasm, source)?;
        wasm = portable::artifact_identity::annotate_ir(&wasm)?;
        require(
            source_section::preserved(&wasm)? == Some(source.as_bytes()),
            "source facts changed during artifact resealing",
        )?;
    }
    portable::artifact_identity::verify(&wasm, Default::default())?;
    runtime_abi::verify_artifact(&wasm, &runtime_abi::Manifest::default())?;
    let module = Module::new(store.engine(), wasm)?;
    Ok(linker.instantiate(store, &module)?)
}
fn execute(e: &Envelope, engine: &Engine) -> Result<u64> {
    require(
        e.verified_mlir
            && ((e.schema == "suss.mlir.bridge.v1" && e.source_analysis.is_none())
                || (e.schema == "suss.mlir.bridge.source.v2" && e.source_analysis.is_some())),
        "requires v1 verified MLIR export",
    )?;
    require(
        e.producer.parameters.is_empty(),
        "producer entry must be closed",
    )?;
    let mut env = Environment::default();
    let id = env.declare_cell(Phase::Runtime, "bridge", "producer")?;
    let mut budget = 8192;
    let (producer, ty) = reconstruct(&e.producer, None, 0, &mut budget)?;
    require(
        e.caller.parameters == vec![ty],
        "caller binding type differs from producer result",
    )?;
    let (caller, result) = reconstruct(&e.caller, Some(&id), 0, &mut budget)?;
    require(result == (Ty::F64 {}), "caller must return f64")?;
    let mut store = Store::new(engine, ());
    store.set_fuel(1_000_000)?;
    let rt = Instance::new(
        &mut store,
        &Module::new(engine, runtime_abi::module())?,
        &[],
    )?;
    let mut linker = Linker::new(engine);
    linker.instance(&mut store, "suss.runtime", rt)?;
    let first = fragment(&mut store, &linker, &producer, e.source_analysis.as_deref())?;
    let closure = call(&mut store, first, "eval", &[])?;
    let cell = call(&mut store, rt, "binding-new", &[closure])?;
    let object = cell
        .unwrap_anyref()
        .ok_or("null cell")?
        .as_struct(&store)?
        .ok_or("cell is not struct")?;
    let ty = object.ty(&store)?;
    let global = Global::new(
        &mut store,
        GlobalType::new(
            ValType::Ref(RefType::new(false, ty.into())),
            Mutability::Const,
        ),
        cell,
    )?;
    linker.define(&store, id.import_module(), &id.import_name(), global)?;
    store.gc(None)?;
    let second = fragment(&mut store, &linker, &caller, e.source_analysis.as_deref())?;
    let value = call(&mut store, second, "eval", &[])?;
    store.gc(None)?;
    let number = value
        .unwrap_anyref()
        .ok_or("null result")?
        .as_struct(&store)?
        .ok_or("result is not number struct")?;
    let fields = number.fields(&mut store)?.collect::<Vec<_>>();
    match fields.as_slice() {
        [Val::F64(bits)] => Ok(*bits),
        _ => Err("independent number decoding failed".into()),
    }
}
fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or(
        "usage: bridge GRAPH.json [MUTATED-VERIFIED-GRAPH.json] | --analyze-source SOURCE.sus",
    )?;
    if comparison::command(&path, &mut args)? {
        return Ok(());
    }
    if path == "--analyze-source" {
        let source_path = args
            .next()
            .ok_or("--analyze-source requires a source file")?;
        require(args.next().is_none(), "unexpected source-analysis argument")?;
        let mut bytes = Vec::new();
        std::fs::File::open(source_path)?
            .take(1_048_577)
            .read_to_end(&mut bytes)?;
        require(bytes.len() <= 1_048_576, "source exceeds 1 MiB")?;
        let source = String::from_utf8(bytes)?;
        let facts = analysis::analyze(&source)?;
        println!("{}", serde_json::to_string_pretty(&facts)?);
        return Ok(());
    }
    if path == "--source-graph" {
        let graph_path = args
            .next()
            .ok_or("--source-graph requires v2 exported graph")?;
        let source_path = args
            .next()
            .ok_or("--source-graph requires original source file")?;
        require(args.next().is_none(), "unexpected source-graph argument")?;
        let read_bounded = |path: String| -> Result<Vec<u8>> {
            let mut bytes = Vec::new();
            std::fs::File::open(path)?
                .take(1_048_577)
                .read_to_end(&mut bytes)?;
            require(bytes.len() <= 1_048_576, "source-graph input exceeds 1 MiB")?;
            Ok(bytes)
        };
        let exported: Envelope = serde_json::from_slice(&read_bounded(graph_path)?)?;
        require(
            exported.schema == "suss.mlir.bridge.source.v2",
            "source graph requires explicit v2 schema",
        )?;
        let source = String::from_utf8(read_bounded(source_path)?)?;
        let facts = analysis::analyze(&source)?;
        analysis::compare_transport(
            &facts,
            exported
                .source_analysis
                .as_ref()
                .ok_or("source facts missing")?
                .as_bytes(),
        )?;
        let mut config = Config::new();
        config
            .wasm_gc(true)
            .wasm_function_references(true)
            .wasm_tail_call(true)
            .wasm_exceptions(true)
            .consume_fuel(true)
            .cranelift_opt_level(wasmtime::OptLevel::None);
        let engine = Engine::new(&config)?;
        abi_probe::verify_before_initializer(&engine)?;
        let actual = execute(&exported, &engine)?;
        require(
            actual == bits(&exported.expected_bits)?,
            "source graph actual result differs",
        )?;
        println!("source_facts_match_genuine_analysis=true");
        println!("source_facts_preserved_in_both_wasm_fragments=true");
        println!("actual_bits={actual:016x}");
        return Ok(());
    }
    if path == "--effects" {
        let graph_path = args.next().ok_or("--effects requires an exported graph")?;
        require(args.next().is_none(), "unexpected effects argument")?;
        let mut bytes = Vec::new();
        std::fs::File::open(graph_path)?
            .take(1_048_577)
            .read_to_end(&mut bytes)?;
        require(bytes.len() <= 1_048_576, "JSON exceeds 1 MiB")?;
        let mut config = Config::new();
        config
            .wasm_gc(true)
            .wasm_function_references(true)
            .wasm_tail_call(true)
            .wasm_exceptions(true)
            .consume_fuel(true)
            .cranelift_opt_level(wasmtime::OptLevel::None);
        let engine = Engine::new(&config)?;
        abi_probe::verify_before_initializer(&engine)?;
        let outcome = effects::execute(&bytes, &engine)?;
        let cells = outcome
            .cells
            .iter()
            .map(|cell| {
                serde_json::json!({
                    "namespace":cell.namespace,"name":cell.name,"bits":format!("{:016x}",cell.bits)
                })
            })
            .collect::<Vec<_>>();
        println!(
            "{}",
            serde_json::json!({
                "schema":"suss.mlir.effects.observation.v1",
                "abi_initializer_gate":true,
                "result_bits":format!("{:016x}",outcome.result_bits),"cells":cells
            })
        );
        return Ok(());
    }
    let mutation = args.next();
    require(args.next().is_none(), "unexpected argument")?;
    let read = |path: String| -> Result<Envelope> {
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(1_048_577)
            .read_to_end(&mut bytes)?;
        require(bytes.len() <= 1_048_576, "JSON exceeds 1 MiB")?;
        Ok(serde_json::from_slice(&bytes)?)
    };
    let original = read(path)?;
    require(
        original.schema == "suss.mlir.bridge.v1",
        "use --source-graph for source-preserving v2",
    )?;
    let mut config = Config::new();
    config
        .wasm_gc(true)
        .wasm_function_references(true)
        .wasm_tail_call(true)
        .wasm_exceptions(true)
        .consume_fuel(true)
        .cranelift_opt_level(wasmtime::OptLevel::None);
    let engine = Engine::new(&config)?;
    abi_probe::verify_before_initializer(&engine)?;
    println!("abi_rejection_before_initializer=passed");
    let actual = execute(&original, &engine)?;
    require(
        actual == bits(&original.expected_bits)?,
        "actual original bits mismatch",
    )?;
    println!("actual_bits={actual:016x}");
    if let Some(path) = mutation {
        let mutated = read(path)?;
        require(
            mutated.schema == "suss.mlir.bridge.v1",
            "mutation requires numeric v1",
        )?;
        let changed = execute(&mutated, &engine)?;
        require(
            changed == bits(&mutated.expected_bits)?,
            "actual mutation bits mismatch",
        )?;
        require(
            changed != actual,
            "mutation did not change actual WasmGC result",
        )?;
        println!("mutation_actual_bits={changed:016x}");
    }
    Ok(())
}

#[cfg(test)]
mod numeric_type_tests {
    use super::{Envelope, Ty};
    #[test]
    fn source_carrier_rejects_null_instead_of_extending_v1_profile() {
        let mut original: serde_json::Value = serde_json::from_str(include_str!(
            "../export-fixtures/exported-original.json"
        )).unwrap();
        assert!(serde_json::from_value::<Envelope>(original.clone()).unwrap().source_analysis.is_none());
        original["source_analysis"] = serde_json::Value::Null;
        assert!(serde_json::from_value::<Envelope>(original.clone()).is_err());
        original["schema"] = "suss.mlir.bridge.source.v2".into();
        assert!(serde_json::from_value::<Envelope>(original.clone()).is_err());
        original["source_analysis"] = "{}".into();
        assert_eq!(serde_json::from_value::<Envelope>(original).unwrap().source_analysis.as_deref(), Some("{}"));
    }
    #[test]
    fn f64_type_rejects_unknown_fields_including_nested_captures() {
        assert!(serde_json::from_str::<Ty>(r#"{"kind":"f64"}"#).is_ok());
        for invalid in [
            r#"{"kind":"f64","extra":true}"#,
            r#"{"kind":"closure","arity":0,"captures":[{"kind":"f64","extra":true}]}"#,
        ] {
            assert!(
                serde_json::from_str::<Ty>(invalid).is_err(),
                "accepted {invalid}"
            );
        }
    }
}
