//! Bounded effects transport consumer. Compilation/execution explicitly pending.
//! Original isolated evaluation code; repository MIT/Apache-2.0 terms.
use super::{Result, bits, call, fragment, require};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use suss_compile::{
    portable::{
        hir::{Arithmetic, Comparison, Literal, Type},
        ir::{self, ValueId},
        resolve::{Environment, Global as CellId, Phase},
    },
    runtime_abi,
};
use wasmtime::{
    Engine, Global, GlobalType, Instance, Linker, Module, Mutability, RefType, Store, Val, ValType,
};

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
// Actual isolated run: 9 passed / 2 failed; the scalar-extra-field rejection
// failed because serde tagged unit variants ignored extras. Keep that regression.
// Empty struct variants preserve the JSON shape and enforce exact scalar keys.
enum Ty {
    #[serde(rename = "f64")]
    F64 {},
    #[serde(rename = "bool")]
    Bool {},
    #[serde(rename = "value")]
    Value {},
    #[serde(rename = "effect_closure")]
    Closure { arity: usize, captures: Vec<Ty> },
}
impl Ty {
    fn physical(&self) -> Type {
        match self {
            Self::F64 {} => Type::Number,
            Self::Bool {} => Type::Bool,
            Self::Value {} => Type::Value,
            Self::Closure { arity, .. } => Type::Closure(*arity),
        }
    }
    fn scalar(&self) -> bool {
        !matches!(self, Self::Closure { .. })
    }
    fn valid(&self) -> Result<()> {
        if let Self::Closure { arity, captures } = self {
            require(
                *arity <= 32 && captures.len() <= 32 && captures.iter().all(Self::scalar),
                "invalid closure type/bounds",
            )?;
        }
        Ok(())
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Parameter {
    id: usize,
    #[serde(rename = "type")]
    ty: Ty,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Region {
    parameters: Vec<Parameter>,
    operations: Vec<Op>,
    terminator: End,
}
#[derive(Deserialize)]
#[serde(tag = "op", deny_unknown_fields)]
enum End {
    #[serde(rename = "return")]
    Return { value: usize },
    #[serde(rename = "throw")]
    Throw { value: usize },
    #[serde(rename = "yield")]
    Yield { value: usize },
}
macro_rules! operations {
 ($($variant:ident => $tag:literal { $($field:ident : $ty:ty),* $(,)? }),* $(,)?) => {
  #[derive(Deserialize)] #[serde(tag="op", deny_unknown_fields)]
  enum Op { $(#[serde(rename=$tag)] $variant { id: usize, result_type: Ty, $($field:$ty),* }),* }
  impl Op { fn header(&self)->(usize,&Ty) { match self { $(Self::$variant { id,result_type,.. } => (*id,result_type)),* } } }
 }
}
operations! {
 Const=>"const" {bits:String}, Bool=>"bool" {value:bool}, Add=>"add" {lhs:usize,rhs:usize},
 Dynamic=>"dynamic_arithmetic" {kind:String,lhs:usize,rhs:usize}, Equal=>"equal" {lhs:usize,rhs:usize},
 Read=>"read" {namespace:String,name:String}, Write=>"write" {namespace:String,name:String,value:usize},
 Closure=>"closure" {captures:Vec<usize>,function:Region}, Call=>"call" {callee:usize,arguments:Vec<usize>},
 If=>"if" {condition:usize,consequent:Region,alternative:Region}, Try=>"try" {body:usize,handler:usize,cleanup:usize},
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Cell {
    namespace: String,
    name: String,
    initial_bits: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    schema: String,
    cells: Vec<Cell>,
    entry: Region,
}

#[derive(Clone)]
struct Binding {
    value: ValueId,
    ty: Ty,
}
type Scope = HashMap<usize, Binding>;
type Cells = HashMap<(String, String), CellId>;
struct Budget(usize);
impl Budget {
    fn take(&mut self) -> Result<()> {
        self.0 = self
            .0
            .checked_sub(1)
            .ok_or("effects total value limit exceeded")?;
        Ok(())
    }
}
struct Builder {
    function: ir::Function,
    allocated: HashSet<usize>,
}
impl Builder {
    fn new() -> Self {
        Self {
            function: ir::Function {
                values: vec![],
                blocks: vec![],
                span: 0..0,
            },
            allocated: HashSet::new(),
        }
    }
    fn value(&mut self, ty: Type) -> ValueId {
        let id = ValueId(self.function.values.len());
        self.function.values.push(ir::Value { ty, span: 0..0 });
        id
    }
    // Placeholder terminators are replaced before the function leaves this builder.
    fn block(&mut self, parameters: Vec<ValueId>) -> usize {
        let id = self.function.blocks.len();
        self.function.blocks.push(ir::Block {
            parameters,
            instructions: vec![],
            terminator: ir::Terminator::Return(ValueId(usize::MAX)),
        });
        id
    }
    fn emit(&mut self, block: usize, ty: Type, operation: ir::Operation) -> ValueId {
        let result = self.value(ty);
        self.function.blocks[block]
            .instructions
            .push(ir::Instruction {
                result,
                operation,
                span: 0..0,
            });
        result
    }
    fn claim(&mut self, id: usize, budget: &mut Budget) -> Result<()> {
        budget.take()?;
        require(
            self.allocated.insert(id),
            "duplicate logical ID within function",
        )
    }
    fn get(scope: &Scope, id: usize) -> Result<Binding> {
        scope
            .get(&id)
            .cloned()
            .ok_or_else(|| "logical operand does not dominate use".into())
    }
    fn cell(cells: &Cells, namespace: &str, name: &str) -> Result<CellId> {
        cells
            .get(&(namespace.into(), name.into()))
            .cloned()
            .ok_or_else(|| "undeclared effects cell".into())
    }
    fn function(
        r: &Region,
        entry: &[Ty],
        depth: usize,
        budget: &mut Budget,
        cells: &Cells,
    ) -> Result<ir::Function> {
        require(
            depth <= 16 && r.parameters.len() <= 64 && r.parameters.len() == entry.len(),
            "effects entry/depth bound or shape mismatch",
        )?;
        let mut b = Self::new();
        let mut scope = Scope::new();
        let mut parameters = vec![];
        for (p, expected) in r.parameters.iter().zip(entry) {
            p.ty.valid()?;
            require(&p.ty == expected, "closure capture/argument entry mismatch")?;
            b.claim(p.id, budget)?;
            let v = b.value(p.ty.physical());
            parameters.push(v);
            scope.insert(
                p.id,
                Binding {
                    value: v,
                    ty: p.ty.clone(),
                },
            );
        }
        let first = b.block(parameters);
        b.region(r, first, scope, depth, None, budget, cells)?;
        Ok(b.function)
    }
    fn region(
        &mut self,
        r: &Region,
        mut block: usize,
        mut scope: Scope,
        depth: usize,
        join: Option<usize>,
        budget: &mut Budget,
        cells: &Cells,
    ) -> Result<usize> {
        require(
            depth <= 16 && r.operations.len() <= 4096,
            "effects region limit exceeded",
        )?;
        if join.is_some() {
            require(r.parameters.is_empty(), "if arms cannot have parameters")?;
        }
        for op in &r.operations {
            let (id, declared) = op.header();
            declared.valid()?;
            self.claim(id, budget)?;
            let mut ty = declared.clone();
            let (operation, physical) = match op {
                Op::Const { bits: raw, .. } => {
                    require(declared == &(Ty::F64 {}), "const result type")?;
                    (
                        ir::Operation::Literal(Literal::Number(f64::from_bits(bits(raw)?))),
                        Type::Number,
                    )
                }
                Op::Bool { value, .. } => {
                    require(declared == &(Ty::Bool {}), "bool result type")?;
                    (ir::Operation::Literal(Literal::Bool(*value)), Type::Bool)
                }
                Op::Add { lhs, rhs, .. } | Op::Dynamic { lhs, rhs, .. } => {
                    let l = Self::get(&scope, *lhs)?;
                    let r = Self::get(&scope, *rhs)?;
                    let (operator, physical) = if let Op::Dynamic { kind, .. } = op {
                        require(
                            matches!(l.ty, Ty::F64 {} | Ty::Value {})
                                && matches!(r.ty, Ty::F64 {} | Ty::Value {})
                                && (l.ty == (Ty::Value {}) || r.ty == (Ty::Value {}))
                                && declared == &(Ty::Value {}),
                            "dynamic arithmetic types",
                        )?;
                        match kind.as_str() {
                            "add" => (Arithmetic::Add, Type::Value),
                            "multiply" => (Arithmetic::Multiply, Type::Number),
                            _ => return Err("unsupported arithmetic kind".into()),
                        }
                    } else {
                        require(
                            l.ty == (Ty::F64 {})
                                && r.ty == (Ty::F64 {})
                                && declared == &(Ty::F64 {}),
                            "add types",
                        )?;
                        (Arithmetic::Add, Type::Number)
                    };
                    (
                        ir::Operation::Arithmetic {
                            operator,
                            arguments: vec![l.value, r.value],
                        },
                        physical,
                    )
                }
                Op::Equal { lhs, rhs, .. } => {
                    let l = Self::get(&scope, *lhs)?;
                    let r = Self::get(&scope, *rhs)?;
                    require(
                        l.ty.scalar() && r.ty.scalar() && declared == &(Ty::Bool {}),
                        "equal types",
                    )?;
                    (
                        ir::Operation::Comparison {
                            operation: Comparison::StrictEqual,
                            arguments: vec![l.value, r.value],
                        },
                        Type::Bool,
                    )
                }
                Op::Read {
                    namespace, name, ..
                } => {
                    require(declared == &(Ty::Value {}), "read result must be Value")?;
                    (
                        ir::Operation::GlobalRead(Self::cell(cells, namespace, name)?),
                        Type::Value,
                    )
                }
                Op::Write {
                    namespace,
                    name,
                    value,
                    ..
                } => {
                    let v = Self::get(&scope, *value)?;
                    require(v.ty.scalar() && declared == &(Ty::Value {}), "write types")?;
                    (
                        ir::Operation::GlobalWrite {
                            global: Self::cell(cells, namespace, name)?,
                            value: v.value,
                        },
                        Type::Value,
                    )
                }
                Op::Closure {
                    captures, function, ..
                } => {
                    let Ty::Closure {
                        arity,
                        captures: ct,
                    } = declared
                    else {
                        return Err("closure result type".into());
                    };
                    let captured = captures
                        .iter()
                        .map(|id| Self::get(&scope, *id))
                        .collect::<Result<Vec<_>>>()?;
                    require(
                        captured.len() == ct.len()
                            && captured
                                .iter()
                                .zip(ct)
                                .all(|(v, t)| &v.ty == t && v.ty.scalar()),
                        "closure capture mismatch",
                    )?;
                    let mut entry = ct.clone();
                    entry.extend(std::iter::repeat_n(Ty::Value {}, *arity));
                    let inner = Self::function(function, &entry, depth + 1, budget, cells)?;
                    let physical_captures = captured
                        .iter()
                        .map(|v| self.function.values[v.value.0].ty)
                        .collect();
                    (
                        ir::Operation::MakeClosure {
                            body: Box::new(ir::ClosureBody {
                                display_name: None,
                                variadic: false,
                                capture_types: physical_captures,
                                arity: *arity,
                                function: inner,
                            }),
                            captures: captured.iter().map(|v| v.value).collect(),
                        },
                        Type::Closure(*arity),
                    )
                }
                Op::Call {
                    callee, arguments, ..
                } => {
                    let c = Self::get(&scope, *callee)?;
                    let Ty::Closure { arity, .. } = c.ty else {
                        return Err("call requires effect closure".into());
                    };
                    require(
                        arguments.len() == arity
                            && arguments.len() <= 32
                            && declared == &(Ty::Value {}),
                        "call arity/result",
                    )?;
                    let mut operands = vec![c.value];
                    for a in arguments {
                        let v = Self::get(&scope, *a)?;
                        require(v.ty.scalar(), "call argument must be scalar")?;
                        operands.push(v.value);
                    }
                    (ir::Operation::Call { operands }, Type::Value)
                }
                Op::Try {
                    body,
                    handler,
                    cleanup,
                    ..
                } => {
                    let regions = [
                        Self::get(&scope, *body)?,
                        Self::get(&scope, *handler)?,
                        Self::get(&scope, *cleanup)?,
                    ];
                    require(
                        declared == &(Ty::Value {})
                            && regions
                                .iter()
                                .zip([0, 1, 0])
                                .all(|(v, a)| matches!(&v.ty,Ty::Closure{arity,..} if *arity==a)),
                        "try requires 0/1/0 closures and Value result",
                    )?;
                    (
                        ir::Operation::Try {
                            regions: regions.map(|v| v.value),
                        },
                        Type::Value,
                    )
                }
                Op::If {
                    condition,
                    consequent,
                    alternative,
                    ..
                } => {
                    let condition = Self::get(&scope, *condition)?;
                    require(
                        condition.ty == (Ty::Bool {}) && declared == &(Ty::Value {}),
                        "if types",
                    )?;
                    let yes = self.block(vec![]);
                    let no = self.block(vec![]);
                    let result = self.value(Type::Value);
                    let continuation = self.block(vec![result]);
                    self.function.blocks[block].terminator = ir::Terminator::Branch {
                        condition: condition.value,
                        consequent: yes,
                        alternative: no,
                    };
                    self.region(
                        consequent,
                        yes,
                        scope.clone(),
                        depth + 1,
                        Some(continuation),
                        budget,
                        cells,
                    )?;
                    self.region(
                        alternative,
                        no,
                        scope.clone(),
                        depth + 1,
                        Some(continuation),
                        budget,
                        cells,
                    )?;
                    block = continuation;
                    scope.insert(
                        id,
                        Binding {
                            value: result,
                            ty: Ty::Value {},
                        },
                    );
                    continue;
                }
            };
            let mut result = self.emit(block, physical, operation);
            // Multiply physically returns Number in public IR. Box via an accepted
            // Number -> Value edge, retaining transport Value without unchecked narrowing.
            if declared == &(Ty::Value {}) && physical != Type::Value {
                let boxed = self.value(Type::Value);
                let continuation = self.block(vec![boxed]);
                self.function.blocks[block].terminator = ir::Terminator::Jump {
                    target: continuation,
                    arguments: vec![result],
                };
                block = continuation;
                result = boxed;
                ty = Ty::Value {};
            }
            scope.insert(id, Binding { value: result, ty });
        }
        let value = match &r.terminator {
            End::Return { value } | End::Throw { value } | End::Yield { value } => {
                Self::get(&scope, *value)?
            }
        };
        self.function.blocks[block].terminator = match (&r.terminator, join) {
            (End::Yield { .. }, Some(target)) => {
                require(value.ty.scalar(), "yield must be scalar")?;
                ir::Terminator::Jump {
                    target,
                    arguments: vec![value.value],
                }
            }
            (End::Return { .. }, None) => {
                require(value.ty.scalar(), "return must be scalar")?;
                ir::Terminator::Return(value.value)
            }
            (End::Throw { .. }, None) => {
                require(value.ty == (Ty::F64 {}), "throw requires f64")?;
                ir::Terminator::Throw(value.value)
            }
            _ => return Err("terminator context mismatch".into()),
        };
        Ok(block)
    }
}

/// Ordered exactly as cells in the transport, not sorted by namespace/name.
#[derive(Debug)]
pub struct CellSnapshot {
    pub namespace: String,
    pub name: String,
    pub bits: u64,
}
#[derive(Debug)]
pub struct Outcome {
    pub result_bits: u64,
    pub cells: Vec<CellSnapshot>,
}
fn singleton(operation: ir::Operation, ty: Type) -> ir::Function {
    ir::Function {
        values: vec![ir::Value { ty, span: 0..0 }],
        blocks: vec![ir::Block {
            parameters: vec![],
            instructions: vec![ir::Instruction {
                result: ValueId(0),
                operation,
                span: 0..0,
            }],
            terminator: ir::Terminator::Return(ValueId(0)),
        }],
        span: 0..0,
    }
}
fn number_bits(store: &mut Store<()>, value: &Val) -> Result<u64> {
    let object = value
        .unwrap_anyref()
        .ok_or("numeric observation is null")?
        .as_struct(&*store)?
        .ok_or("numeric observation is not a struct")?;
    let fields = object.fields(&mut *store)?.collect::<Vec<_>>();
    match fields.as_slice() {
        [Val::F64(bits)] => Ok(*bits),
        _ => Err("numeric observation is not a boxed f64".into()),
    }
}
/// Consume exported graph bytes. Caller supplies the configured GC/EH/fuel engine
/// and performs its ABI gate. This API does not compile original HIR/source.
/// Numeric result/snapshot decoding rejects nonnumeric observations explicitly.
/// Unhandled guest throws propagate as errors, never successful fake values.
fn prepare(bytes: &[u8]) -> Result<(Envelope, Cells, ir::Function)> {
    require(bytes.len() <= 1_048_576, "effects JSON exceeds 1 MiB")?;
    let UniqueJson(value) = serde_json::from_slice::<UniqueJson>(bytes)?;
    let e: Envelope = serde_json::from_value(value)?;
    require(
        e.schema == "suss.mlir.effects.v1" && e.cells.len() <= 32 && e.entry.parameters.is_empty(),
        "effects envelope/entry mismatch",
    )?;
    let mut env = Environment::default();
    let mut cells = Cells::new();
    for (index, cell) in e.cells.iter().enumerate() {
        bits(&cell.initial_bits)?;
        require(
            !cell.namespace.is_empty() && !cell.name.is_empty(),
            "empty cell identifier",
        )?;
        let key = (cell.namespace.clone(), cell.name.clone());
        require(!cells.contains_key(&key), "duplicate effects cell")?;
        cells.insert(
            key,
            // Transport pairs are opaque, exact identities. Generate private legal
            // import names so resolver canonicalization (cljs.core -> suss.core)
            // cannot merge distinct declared cells or reinterpret their spelling.
            env.declare_cell(Phase::Runtime, "bridge.effects", &format!("cell{index}"))?,
        );
    }
    let function = Builder::function(&e.entry, &[], 0, &mut Budget(8192), &cells)?;
    ir::verify(&function)?; // Independent public verifier before any guest execution.
    Ok((e, cells, function))
}

/// Strict transport validation and actual IR reconstruction, without execution.
pub fn reconstruct(bytes: &[u8]) -> Result<ir::Function> {
    Ok(prepare(bytes)?.2)
}

/// Execute only the verified exported graph. See numeric observation limits above.
pub fn execute(bytes: &[u8], engine: &Engine) -> Result<Outcome> {
    let (e, cells, function) = prepare(bytes)?;
    let mut store = Store::new(engine, ());
    store.set_fuel(1_000_000)?;
    let rt = Instance::new(
        &mut store,
        &Module::new(engine, runtime_abi::module())?,
        &[],
    )?;
    let mut linker = Linker::new(engine);
    linker.instance(&mut store, "suss.runtime", rt)?;
    for cell in &e.cells {
        let initial = singleton(
            ir::Operation::Literal(Literal::Number(f64::from_bits(bits(&cell.initial_bits)?))),
            Type::Number,
        );
        let instance = fragment(&mut store, &linker, &initial, None)?;
        let initial = call(&mut store, instance, "eval", &[])?;
        let value = call(&mut store, rt, "binding-new", &[initial])?;
        let object = value
            .unwrap_anyref()
            .ok_or("null cell")?
            .as_struct(&store)?
            .ok_or("cell is not a struct")?;
        let ty = object.ty(&store)?;
        let global = Global::new(
            &mut store,
            GlobalType::new(
                ValType::Ref(RefType::new(false, ty.into())),
                Mutability::Const,
            ),
            value,
        )?;
        let id = Builder::cell(&cells, &cell.namespace, &cell.name)?;
        linker.define(&store, id.import_module(), &id.import_name(), global)?;
    }
    store.gc(None)?;
    let instance = fragment(&mut store, &linker, &function, None)?;
    let result = call(&mut store, instance, "eval", &[])?;
    store.gc(None)?;
    let result_bits = number_bits(&mut store, &result)?;
    let mut snapshots = vec![];
    for cell in e.cells {
        let read = singleton(
            ir::Operation::GlobalRead(Builder::cell(&cells, &cell.namespace, &cell.name)?),
            Type::Value,
        );
        let instance = fragment(&mut store, &linker, &read, None)?;
        let value = call(&mut store, instance, "eval", &[])?;
        store.gc(None)?;
        snapshots.push(CellSnapshot {
            namespace: cell.namespace,
            name: cell.name,
            bits: number_bits(&mut store, &value)?,
        });
    }
    Ok(Outcome {
        result_bits,
        cells: snapshots,
    })
}

// Reject duplicate decoded keys before typed deserialization.
struct UniqueJson(serde_json::Value);
impl<'de> serde::Deserialize<'de> for UniqueJson {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        struct UniqueVisitor;
        impl<'de> serde::de::Visitor<'de> for UniqueVisitor {
            type Value = UniqueJson;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("JSON with unique decoded object keys")
            }
            fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(serde_json::Value::Null))
            }
            fn visit_bool<E: serde::de::Error>(
                self,
                v: bool,
            ) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(serde_json::Value::Bool(v)))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(serde_json::Value::Number(v.into())))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(serde_json::Value::Number(v.into())))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> std::result::Result<Self::Value, E> {
                serde_json::Number::from_f64(v)
                    .map(|v| UniqueJson(serde_json::Value::Number(v)))
                    .ok_or_else(|| E::custom("non-finite JSON number"))
            }
            fn visit_str<E: serde::de::Error>(
                self,
                v: &str,
            ) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(serde_json::Value::String(v.to_owned())))
            }
            fn visit_string<E: serde::de::Error>(
                self,
                v: String,
            ) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(serde_json::Value::String(v)))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(UniqueJson(value)) = seq.next_element::<UniqueJson>()? {
                    values.push(value);
                }
                Ok(UniqueJson(serde_json::Value::Array(values)))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                use serde::de::Error as _;
                let mut values = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(A::Error::custom(format!("duplicate JSON key: {key}")));
                    }
                    let UniqueJson(value) = map.next_value::<UniqueJson>()?;
                    values.insert(key, value);
                }
                Ok(UniqueJson(serde_json::Value::Object(values)))
            }
        }
        deserializer.deserialize_any(UniqueVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const SIMPLE: &str = r#"{"schema":"suss.mlir.effects.v1","cells":[],"entry":{"parameters":[],"operations":[{"id":7,"op":"const","result_type":{"kind":"f64"},"bits":"401c000000000000"}],"terminator":{"op":"return","value":7}}}"#;
    #[test]
    fn retained_exported_graphs_reconstruct_real_ir() {
        for bytes in [
            include_bytes!("../effects-fixtures/success.json").as_slice(),
            include_bytes!("../effects-fixtures/alternative.json").as_slice(),
            include_bytes!("../effects-fixtures/cleanup-overrides-success.json").as_slice(),
            include_bytes!("../effects-fixtures/handled-throw.json").as_slice(),
            include_bytes!("../effects-fixtures/captured-f64.json").as_slice(),
            include_bytes!("../effects-fixtures/nested-join.json").as_slice(),
            include_bytes!("../effects-fixtures/cleanup-overrides-handler.json").as_slice(),
        ] {
            let f = reconstruct(bytes).unwrap();
            ir::verify(&f).unwrap();
        }
    }
    #[test]
    fn transport_rejects_unknown_duplicate_forward_and_wrong_type() {
        reconstruct(SIMPLE.as_bytes()).unwrap();
        for bad in [
            SIMPLE.replace("\"cells\":[]", "\"cells\":[],\"extra\":true"),
            SIMPLE.replace("\"id\":7", "\"id\":7,\"id\":7"),
            SIMPLE.replace("\"value\":7", "\"value\":8"),
            SIMPLE.replace("\"kind\":\"f64\"", "\"kind\":\"value\""),
            SIMPLE.replace("401c000000000000", "401C000000000000"),
            SIMPLE.replace("\"op\":\"return\"", "\"op\":\"yield\""),
            SIMPLE.replace("\"kind\":\"f64\"", "\"kind\":\"f64\",\"extra\":true"),
        ] {
            assert!(reconstruct(bad.as_bytes()).is_err(), "accepted {bad}");
        }
    }
    #[test]
    fn every_scalar_type_rejects_extra_fields() {
        for kind in ["f64", "bool", "value"] {
            let valid = format!(r#"{{"kind":"{kind}"}}"#);
            assert!(serde_json::from_str::<Ty>(&valid).is_ok());
            let invalid = format!(r#"{{"kind":"{kind}","extra":true}}"#);
            assert!(
                serde_json::from_str::<Ty>(&invalid).is_err(),
                "accepted {invalid}"
            );
            let nested = format!(r#"{{"kind":"effect_closure","arity":0,"captures":[{invalid}]}}"#);
            assert!(
                serde_json::from_str::<Ty>(&nested).is_err(),
                "accepted {nested}"
            );
        }
    }
    #[test]
    fn rejects_cross_arm_try_capture_cells_and_escaped_duplicates() {
        use serde_json::{Value, json};
        fn edit(
            v: &mut Value,
            predicate: &impl Fn(&Value) -> bool,
            mutation: &mut impl FnMut(&mut Value),
        ) -> bool {
            if predicate(v) {
                mutation(v);
                return true;
            }
            match v {
                Value::Array(a) => a.iter_mut().any(|v| edit(v, predicate, mutation)),
                Value::Object(o) => o.values_mut().any(|v| edit(v, predicate, mutation)),
                _ => false,
            }
        }
        fn rejects(v: &Value) {
            assert!(reconstruct(&serde_json::to_vec(v).unwrap()).is_err());
        }
        let base: Value =
            serde_json::from_slice(include_bytes!("../effects-fixtures/success.json")).unwrap();
        let mut cross = base.clone();
        assert!(edit(&mut cross, &|v| v["op"] == "if", &mut |v| {
            let id = v["consequent"]["operations"]
                .as_array()
                .unwrap()
                .last()
                .unwrap()["id"]
                .clone();
            v["alternative"]["terminator"]["value"] = id;
        }));
        rejects(&cross);
        let mut wrong_try = base.clone();
        assert!(edit(&mut wrong_try, &|v| v["op"] == "try", &mut |v| {
            v["handler"] = v["cleanup"].clone();
        }));
        rejects(&wrong_try);
        let mut undeclared = base.clone();
        assert!(edit(&mut undeclared, &|v| v["op"] == "read", &mut |v| v
            ["name"] =
            json!("missing")));
        rejects(&undeclared);
        let mut duplicate = base.clone();
        let cell = duplicate["cells"][0].clone();
        duplicate["cells"].as_array_mut().unwrap().push(cell);
        rejects(&duplicate);
        let mut capture: Value =
            serde_json::from_slice(include_bytes!("../effects-fixtures/captured-f64.json"))
                .unwrap();
        assert!(edit(
            &mut capture,
            &|v| v["op"] == "closure" && v["captures"].as_array().is_some_and(|a| !a.is_empty()),
            &mut |v| {
                v["function"]["parameters"][0]["type"] = json!({"kind":"value"});
            }
        ));
        rejects(&capture);
        let escaped = SIMPLE.replace("\"id\":7", r#""id":7,"\u0069d":7"#);
        assert!(reconstruct(escaped.as_bytes()).is_err());
    }
    #[test]
    fn multiply_and_if_use_value_edges_without_id_aliasing() {
        let f = reconstruct(include_bytes!("../effects-fixtures/success.json")).unwrap();
        let mut edges = 0;
        fn inspect(f: &ir::Function, edges: &mut usize) {
            for b in &f.blocks {
                if let ir::Terminator::Jump { target, arguments } = &b.terminator {
                    for (v, p) in arguments.iter().zip(&f.blocks[*target].parameters) {
                        if f.values[p.0].ty == Type::Value && f.values[v.0].ty == Type::Number {
                            *edges += 1;
                        }
                    }
                }
                for i in &b.instructions {
                    if let ir::Operation::MakeClosure { body, .. } = &i.operation {
                        inspect(&body.function, edges);
                    }
                }
            }
        }
        inspect(&f, &mut edges);
        assert!(edges >= 2);
    }
}
