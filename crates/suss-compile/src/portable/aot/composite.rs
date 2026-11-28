//! Owned composite adapters generated from the resolved WIT graph.
use super::{Diagnostic, Scalar, error};
use std::collections::HashMap;
use wasm_encoder::*;
use wit_parser::abi::{FlatTypes, WasmType};
use wit_parser::{Int, Resolve, SizeAlign, Type, TypeDefKind};

#[derive(Clone)]
pub(super) enum Shape {
    Scalar(Scalar),
    String,
    Product {
        fields: Vec<(Option<String>, u32, usize)>,
        record: bool,
    },
    Variant {
        tag: Int,
        payload: u32,
        cases: Vec<(String, Option<usize>)>,
    },
    List {
        map: bool,
        element: usize,
        stride: u32,
    },
}

#[derive(Clone)]
pub(super) struct Node {
    /// Fixed lists share product layout, but retain numeric packed-array rules.
    pub fixed_element: Option<usize>,
    /// Enum keywords and flag sets use scalar canonical storage.
    pub symbols: Option<(bool, Vec<String>)>,
    pub shape: Shape,
    pub flat: Vec<ValType>,
    pub size: u32,
    pub align: u32,
    pub name: String,
    pub dynamic: bool,
}

pub(super) struct Registry<'a> {
    pub resolve: &'a Resolve,
    sizes: SizeAlign,
    ids: HashMap<Type, usize>,
    pub nodes: Vec<Node>,
}

fn normalized(resolve: &Resolve, mut ty: Type) -> Type {
    while let Type::Id(id) = ty {
        if let TypeDefKind::Type(next) = resolve.types[id].kind {
            ty = next;
        } else {
            break;
        }
    }
    ty
}

pub(super) fn flat(resolve: &Resolve, ty: Type) -> Vec<ValType> {
    let mut storage = vec![WasmType::I32; 16];
    loop {
        let mut result = FlatTypes::new(&mut storage);
        if resolve.push_flat(&ty, &mut result) {
            return result
                .to_vec()
                .into_iter()
                .map(|ty| match ty {
                    WasmType::I32 | WasmType::Pointer | WasmType::Length => ValType::I32,
                    WasmType::I64 | WasmType::PointerOrI64 => ValType::I64,
                    WasmType::F32 => ValType::F32,
                    WasmType::F64 => ValType::F64,
                })
                .collect();
        }
        storage.resize(
            storage.len().checked_mul(2).expect("bounded WIT type size"),
            WasmType::I32,
        );
    }
}

impl<'a> Registry<'a> {
    pub fn new(resolve: &'a Resolve) -> Self {
        let mut sizes = SizeAlign::default();
        sizes.fill(resolve);
        Self {
            resolve,
            sizes,
            ids: HashMap::new(),
            nodes: vec![],
        }
    }
    pub fn index(&self, ty: Type) -> usize {
        self.ids[&normalized(self.resolve, ty)]
    }
    pub fn add(&mut self, ty: Type) -> Result<usize, Diagnostic> {
        self.add_at(ty, 0)
    }
    fn add_at(&mut self, ty: Type, depth: usize) -> Result<usize, Diagnostic> {
        if depth >= 64 {
            return Err(error("WIT composite nesting exceeds 64"));
        }
        let ty = normalized(self.resolve, ty);
        if let Some(index) = self.ids.get(&ty) {
            return Ok(*index);
        }
        let size = u32::try_from(self.sizes.size(&ty).size_wasm32())
            .map_err(|_| error("WIT value exceeds memory32"))?;
        let align = self.sizes.align(&ty).align_wasm32() as u32;
        let name = match ty {
            Type::Id(id) => self.resolve.types[id]
                .name
                .clone()
                .unwrap_or_else(|| self.resolve.types[id].kind.as_str().to_owned()),
            _ => format!("{ty:?}").to_lowercase(),
        };
        let mut symbols = None;
        let mut fixed_element = None;
        let shape = if ty == Type::String {
            Shape::String
        } else if let Ok(scalar) = super::scalar(self.resolve, ty) {
            Shape::Scalar(scalar)
        } else if let Type::Id(id) = ty {
            match self.resolve.types[id].kind.clone() {
                TypeDefKind::Tuple(tuple) => {
                    if tuple.types.is_empty() {
                        return Err(error("WIT tuple requires at least one element"));
                    }
                    let offsets = self
                        .sizes
                        .field_offsets(tuple.types.iter())
                        .into_iter()
                        .map(|(offset, _)| offset.size_wasm32() as u32)
                        .collect::<Vec<_>>();
                    let mut fields = vec![];
                    for (ty, offset) in tuple.types.into_iter().zip(offsets) {
                        fields.push((None, offset, self.add_at(ty, depth + 1)?));
                    }
                    Shape::Product {
                        fields,
                        record: false,
                    }
                }
                TypeDefKind::Record(record) => {
                    if record.fields.is_empty() {
                        return Err(error("WIT record requires at least one field"));
                    }
                    let offsets = self
                        .sizes
                        .field_offsets(record.fields.iter().map(|field| &field.ty))
                        .into_iter()
                        .map(|(offset, _)| offset.size_wasm32() as u32)
                        .collect::<Vec<_>>();
                    let mut fields = vec![];
                    for (field, offset) in record.fields.into_iter().zip(offsets) {
                        fields.push((Some(field.name), offset, self.add_at(field.ty, depth + 1)?));
                    }
                    Shape::Product {
                        fields,
                        record: true,
                    }
                }
                TypeDefKind::Enum(enumeration) => {
                    if enumeration.cases.is_empty() {
                        return Err(error("WIT enum requires at least one case"));
                    }
                    let scalar = tag_scalar(enumeration.tag());
                    symbols = Some((
                        false,
                        enumeration
                            .cases
                            .into_iter()
                            .map(|case| case.name)
                            .collect(),
                    ));
                    Shape::Scalar(scalar)
                }
                TypeDefKind::Flags(flags) => {
                    let len = flags.flags.len();
                    if !(1..=32).contains(&len) {
                        return Err(error(
                            "Pinned canonical ABI requires between one and 32 flags",
                        ));
                    }
                    symbols = Some((
                        true,
                        flags.flags.into_iter().map(|flag| flag.name).collect(),
                    ));
                    Shape::Scalar(if len <= 8 {
                        Scalar::U8
                    } else if len <= 16 {
                        Scalar::U16
                    } else {
                        Scalar::U32
                    })
                }
                TypeDefKind::Variant(variant) => {
                    if variant.cases.is_empty() {
                        return Err(error("WIT variant requires at least one case"));
                    }
                    let tag = variant.tag();
                    let payload = self
                        .sizes
                        .payload_offset(tag, variant.cases.iter().map(|case| case.ty.as_ref()))
                        .size_wasm32() as u32;
                    let cases = variant
                        .cases
                        .into_iter()
                        .map(|case| {
                            Ok((
                                case.name,
                                case.ty.map(|ty| self.add_at(ty, depth + 1)).transpose()?,
                            ))
                        })
                        .collect::<Result<_, Diagnostic>>()?;
                    Shape::Variant {
                        tag,
                        payload,
                        cases,
                    }
                }
                TypeDefKind::Option(payload) => {
                    let offset = self
                        .sizes
                        .payload_offset(Int::U8, [Some(&payload)])
                        .size_wasm32() as u32;
                    Shape::Variant {
                        tag: Int::U8,
                        payload: offset,
                        cases: vec![
                            ("none".into(), None),
                            ("some".into(), Some(self.add_at(payload, depth + 1)?)),
                        ],
                    }
                }
                TypeDefKind::Result(result) => {
                    let offset = self
                        .sizes
                        .payload_offset(Int::U8, [result.ok.as_ref(), result.err.as_ref()])
                        .size_wasm32() as u32;
                    let ok = result.ok.map(|ty| self.add_at(ty, depth + 1)).transpose()?;
                    let err = result
                        .err
                        .map(|ty| self.add_at(ty, depth + 1))
                        .transpose()?;
                    Shape::Variant {
                        tag: Int::U8,
                        payload: offset,
                        cases: vec![("ok".into(), ok), ("err".into(), err)],
                    }
                }
                TypeDefKind::FixedLengthList(element, length) => {
                    if length == 0 {
                        return Err(error("WIT fixed-length list requires at least one element"));
                    }
                    let child = self.add_at(element, depth + 1)?;
                    fixed_element = Some(child);
                    let stride = self.nodes[child].size;
                    Shape::Product {
                        record: false,
                        fields: (0..length).map(|i| (None, i * stride, child)).collect(),
                    }
                }
                TypeDefKind::Map(key, value) => {
                    let key_id = self.add_at(key, depth + 1)?;
                    let value_id = self.add_at(value, depth + 1)?;
                    let types = [key, value];
                    let offsets = self.sizes.field_offsets(types.iter());
                    let layout = self.sizes.record(types.iter());
                    let stride = u32::try_from(layout.size.size_wasm32())
                        .map_err(|_| error("WIT map stride exceeds memory32"))?;
                    let element = self.nodes.len();
                    self.nodes.push(Node {
                        fixed_element: None,
                        symbols: None,
                        shape: Shape::Product {
                            record: false,
                            fields: vec![
                                (None, 0, key_id),
                                (None, offsets[1].0.size_wasm32() as u32, value_id),
                            ],
                        },
                        flat: self.nodes[key_id]
                            .flat
                            .iter()
                            .chain(&self.nodes[value_id].flat)
                            .copied()
                            .collect(),
                        size: stride,
                        align: layout.align.align_wasm32() as u32,
                        name: format!("map-entry-{element}"),
                        dynamic: self.nodes[key_id].dynamic || self.nodes[value_id].dynamic,
                    });
                    Shape::List {
                        map: true,
                        element,
                        stride,
                    }
                }
                TypeDefKind::List(element) => {
                    let stride = u32::try_from(self.sizes.size(&element).size_wasm32())
                        .map_err(|_| error("WIT list stride exceeds memory32"))?;
                    Shape::List {
                        map: false,
                        element: self.add_at(element, depth + 1)?,
                        stride,
                    }
                }
                _ => {
                    return Err(error(format!(
                        "Portable AOT {} boundary adapters remain unimplemented",
                        super::boundary_description(self.resolve, ty, 0)
                    )));
                }
            }
        } else {
            return Err(error(format!("Unsupported WIT value {ty:?}")));
        };
        let dynamic = match &shape {
            Shape::Scalar(_) => false,
            Shape::String | Shape::List { .. } => true,
            Shape::Product { fields, .. } => fields
                .iter()
                .any(|(_, _, child)| self.nodes[*child].dynamic),
            Shape::Variant { cases, .. } => cases
                .iter()
                .any(|(_, child)| child.is_some_and(|id| self.nodes[id].dynamic)),
        };
        let index = self.nodes.len();
        self.nodes.push(Node {
            fixed_element,
            symbols,
            shape,
            flat: flat(self.resolve, ty),
            size,
            align,
            name,
            dynamic,
        });
        self.ids.insert(ty, index);
        Ok(index)
    }

    /// Project the complete source value into private owned arrays before any
    /// canonical allocation. The Wasm validator then checks every leaf first.
    fn packed(&self, element: usize) -> Option<(Scalar, u32)> {
        let node = &self.nodes[element];
        if node.symbols.is_some() {
            return None;
        }
        let Shape::Scalar(scalar) = node.shape else {
            return None;
        };
        let kind = match scalar {
            Scalar::S8 | Scalar::U8 => 0,
            Scalar::S16 => 1,
            Scalar::S32 => 2,
            Scalar::F32 => 3,
            Scalar::S64 => 4,
            Scalar::F64 => 5,
            _ => return None,
        };
        Some((scalar, kind))
    }
    pub fn source(&self) -> String {
        let mut source = format!(
            "(let [vs {} same suss.core/identical? eq suss.core/=\n arr suss.core/array get suss.core/aget put suss.core/aset\n alloc (fn [n] (suss.bootstrap/packed-array 6 n))\n keyword-class suss.core/Keyword map? suss.core/map? count suss.core/count has suss.core/contains? lookup suss.core/get assoc suss.core/assoc empty-map {{}} set? suss.core/set? conj suss.core/conj band suss.core/bit-and bor suss.core/bit-or alen suss.core/alength nth suss.core/nth seq suss.core/seq first suss.core/first next suss.core/next key suss.core/key val suss.core/val\n",
            super::list_schema::SOURCE
        );
        for (id, node) in self.nodes.iter().enumerate() {
            let failure = format!(
                "(throw \"WIT {} requires its declared {} shape\")",
                node.name,
                if matches!(node.shape, Shape::Product { record: true, .. }) {
                    "record"
                } else {
                    "value"
                }
            );
            let (lift, project) = if let Some((flags, names)) = &node.symbols {
                source.push_str(&format!(
                    "symbols{id} (arr {})\n",
                    names
                        .iter()
                        .map(|name| format!(":{name}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                ));
                if *flags {
                    let mut lift = "s0 #{} ".to_owned();
                    let mut bindings = String::new();
                    let mut counts = vec!["0".to_owned()];
                    let mut bits = vec!["0".to_owned()];
                    for i in 0..names.len() {
                        let mask = 1_u64 << i;
                        lift.push_str(&format!("s{} (if (eq (band v {mask}) 0) s{i} (conj s{i} (get symbols{id} {i}))) ", i + 1));
                        bindings.push_str(&format!("b{i} (has v (get symbols{id} {i})) "));
                        counts.push(format!("(if b{i} 1 0)"));
                        bits.push(format!("(if b{i} {mask} 0)"));
                    }
                    (
                        format!("(fn [v] (let [{lift}] s{}))", names.len()),
                        format!(
                            "(fn [v] (if (set? v) (let [{bindings}] (if (eq (count v) (suss.core/+ {})) (bor {}) {failure})) {failure}))",
                            counts.join(" "),
                            bits.join(" ")
                        ),
                    )
                } else {
                    let mut lift = failure.clone();
                    let mut project = failure.clone();
                    for i in (0..names.len()).rev() {
                        lift = format!("(if (eq v {i}) (get symbols{id} {i}) {lift})");
                        project = format!("(if (eq v (get symbols{id} {i})) {i} {project})");
                    }
                    (
                        format!("(fn [v] {lift})"),
                        format!(
                            "(fn [v] (if (suss.core/instance? keyword-class v) {project} {failure}))"
                        ),
                    )
                }
            } else {
                match &node.shape {
                    Shape::Scalar(_) | Shape::String => ("(fn [v] v)".into(), "(fn [v] v)".into()),
                    Shape::Product {
                        fields,
                        record: true,
                    } => {
                        source.push_str(&format!(
                            "keys{id} (arr {})\n",
                            fields
                                .iter()
                                .map(|(name, _, _)| format!(":{}", name.as_ref().unwrap()))
                                .collect::<Vec<_>>()
                                .join(" ")
                        ));
                        let mut lift = "empty-map".to_owned();
                        for (i, _) in fields.iter().enumerate() {
                            lift = format!("(assoc {lift} (get keys{id} {i}) (get v {i}))");
                        }
                        let checks = fields
                            .iter()
                            .enumerate()
                            .rev()
                            .fold("true".to_owned(), |rest, (i, _)| {
                                format!("(if (has v (get keys{id} {i})) {rest} false)")
                            });
                        let values = fields
                            .iter()
                            .enumerate()
                            .map(|(i, (_, _, child))| {
                                format!("(p{child} (lookup v (get keys{id} {i})))")
                            })
                            .collect::<Vec<_>>()
                            .join(" ");
                        (
                            format!("(fn [v] {lift})"),
                            format!(
                                "(fn [v] (if (map? v) (if (same (count v) {}) (if {checks} (arr {values}) {failure}) {failure}) {failure}))",
                                fields.len()
                            ),
                        )
                    }
                    Shape::Product {
                        fields,
                        record: false,
                    } => {
                        let values = fields
                            .iter()
                            .enumerate()
                            .map(|(i, (_, _, child))| format!("(p{child} (vs 2 parts {i}))"))
                            .collect::<Vec<_>>()
                            .join(" ");
                        let vector = format!(
                            "(let [parts (vs 1 v 0)] (if (same (vs 3 parts 0) {}) (arr {values}) {failure}))",
                            fields.len()
                        );
                        let project = if let Some((_, kind)) =
                            node.fixed_element.and_then(|child| self.packed(child))
                        {
                            format!(
                                "(fn [v] (let [v (vs 4 v {kind})] (if (suss.bootstrap/packed-array? v) (if (suss.bootstrap/object-array? v) {failure} (if (same (alen v) {}) v {failure})) {vector})))",
                                fields.len()
                            )
                        } else {
                            format!("(fn [v] {vector})")
                        };
                        ("(fn [v] (vs 0 v 0))".into(), project)
                    }
                    Shape::List {
                        map: true, element, ..
                    } => {
                        let Shape::Product { fields, .. } = &self.nodes[*element].shape else {
                            unreachable!()
                        };
                        let key = fields[0].2;
                        let value = fields[1].2;
                        let write = format!(
                            "(put out i (arr (p{key} (key entry)) (p{value} (val entry))))"
                        );
                        let step = format!(
                            "(let [entry (first entries)] {write} (recur (next entries) (suss.core/+ i 1)))"
                        );
                        ("(fn [v] (loop [i 0 out empty-map] (if (suss.core/< i (alen v)) (let [entry (get v i)] (recur (suss.core/+ i 1) (assoc out (nth entry 0) (nth entry 1)))) out)))".into(),
                        format!("(fn [v] (if (map? v) (let [out (alloc (count v))] (loop [entries (seq v) i 0] (if entries {step} out))) {failure}))"))
                    }
                    Shape::List { element, .. } => {
                        let values = format!(
                            "(let [parts (vs 1 v 0) n (vs 3 parts 0) out (alloc n)] (loop [i 0] (if (suss.core/< i n) (do (put out i (p{element} (vs 2 parts i))) (recur (suss.core/+ i 1))) out)))"
                        );
                        let project = if let Some((_, kind)) = self.packed(*element) {
                            format!(
                                "(fn [v] (let [v (vs 4 v {kind})] (if (suss.bootstrap/packed-array? v) (if (suss.bootstrap/object-array? v) {failure} v) {values})))"
                            )
                        } else {
                            format!("(fn [v] {values})")
                        };
                        ("(fn [v] (vs 0 v 0))".into(), project)
                    }
                    Shape::Variant { cases, .. } => {
                        source.push_str(&format!(
                            "tags{id} (arr {})\n",
                            cases
                                .iter()
                                .map(|(name, _)| format!(":{name}"))
                                .collect::<Vec<_>>()
                                .join(" ")
                        ));
                        let mut lift = failure.clone();
                        let mut project = failure.clone();
                        for (i, (_, child)) in cases.iter().enumerate().rev() {
                            let lifted = if child.is_some() {
                                format!("(arr (get tags{id} {i}) (get v 1))")
                            } else {
                                format!("(arr (get tags{id} {i}))")
                            };
                            lift = format!("(if (same (get v 0) {i}) (vs 0 {lifted} 0) {lift})");
                            let projected = child
                                .map(|child| format!("(p{child} (vs 2 parts 1))"))
                                .unwrap_or_else(|| "nil".into());
                            project = format!(
                                "(if (eq tag (get tags{id} {i})) (if (same n {}) (arr {i} {projected}) {failure}) {project})",
                                if child.is_some() { 2 } else { 1 }
                            );
                        }
                        (
                            format!("(fn [v] {lift})"),
                            format!(
                                "(fn [v] (let [parts (vs 1 v 0) n (vs 3 parts 0)] (if (suss.core/> n 0) (let [tag (vs 2 parts 0)] (if (suss.core/instance? keyword-class tag) {project} {failure})) {failure})))"
                            ),
                        )
                    }
                }
            };
            source.push_str(&format!("l{id} {lift}\np{id} {project}\n"));
        }
        source.push_str("] (arr ");
        for id in 0..self.nodes.len() {
            source.push_str(&format!("l{id} p{id} "));
        }
        source.push_str("))");
        source
    }
}

pub(super) const COPY: u32 = 0;
pub(super) const VALIDATE: u32 = 1;
pub(super) const WRITE: u32 = 2;
pub(super) const FREE: u32 = 3;
pub(super) const STORE_FLAT: u32 = 4;
pub(super) const LOAD_FLAT: u32 = 5;
const WIDTH: u32 = 6;
const ARRAY_NEW: u32 = super::HELPERS + 4;
const ARRAY_SET: u32 = super::HELPERS + 5;
const ARRAY_GET: u32 = super::HELPERS + 6;
const ARRAY_LENGTH: u32 = super::HELPERS + 7;

pub(super) fn function(base: u32, node: usize, operation: u32) -> u32 {
    base + node as u32 * WIDTH + operation
}

fn emit(body: &mut Function, instructions: &[Instruction<'_>]) {
    for instruction in instructions {
        body.instruction(instruction);
    }
}
fn memory(offset: u32, align: u32) -> MemArg {
    MemArg {
        offset: u64::from(offset),
        align,
        memory_index: 0,
    }
}
fn load(scalar: Scalar) -> Instruction<'static> {
    use Instruction::*;
    match scalar {
        Scalar::Bool | Scalar::U8 => I32Load8U(memory(0, 0)),
        Scalar::S8 => I32Load8S(memory(0, 0)),
        Scalar::U16 => I32Load16U(memory(0, 1)),
        Scalar::S16 => I32Load16S(memory(0, 1)),
        Scalar::U32 | Scalar::S32 | Scalar::Char => I32Load(memory(0, 2)),
        Scalar::U64 | Scalar::S64 => I64Load(memory(0, 3)),
        Scalar::F32 => F32Load(memory(0, 2)),
        Scalar::F64 => F64Load(memory(0, 3)),
    }
}
fn store(scalar: Scalar) -> Instruction<'static> {
    use Instruction::*;
    match scalar {
        Scalar::Bool | Scalar::U8 | Scalar::S8 => I32Store8(memory(0, 0)),
        Scalar::U16 | Scalar::S16 => I32Store16(memory(0, 1)),
        Scalar::U32 | Scalar::S32 | Scalar::Char => I32Store(memory(0, 2)),
        Scalar::U64 | Scalar::S64 => I64Store(memory(0, 3)),
        Scalar::F32 => F32Store(memory(0, 2)),
        Scalar::F64 => F64Store(memory(0, 3)),
    }
}
fn tag_scalar(tag: Int) -> Scalar {
    match tag {
        Int::U8 => Scalar::U8,
        Int::U16 => Scalar::U16,
        Int::U32 => Scalar::U32,
        Int::U64 => Scalar::U64,
    }
}

struct Locals {
    value: u32,
    tree: u32,
    integer: u32,
    pointer: u32,
    length: u32,
    index: u32,
    unit: u32,
    wide: u32,
    data: u32,
}
impl Locals {
    fn new(parameters: u32) -> Self {
        Self {
            value: parameters,
            tree: parameters + 1,
            integer: parameters + 3,
            pointer: parameters + 4,
            length: parameters + 5,
            index: parameters + 6,
            unit: parameters + 7,
            wide: parameters + 8,
            data: parameters + 9,
        }
    }
    fn declarations() -> Vec<(u32, ValType)> {
        vec![
            (3, super::VALUE),
            (5, ValType::I32),
            (1, ValType::I64),
            (1, ValType::I32),
        ]
    }
    fn string(&self) -> super::strings::Locals {
        super::strings::Locals {
            value: self.value,
            pointer: self.pointer,
            length: self.length,
            index: self.index,
            unit: self.unit,
        }
    }
}
fn strings() -> super::strings::Helpers {
    super::strings::Helpers {
        new: 4,
        length: super::HELPERS,
        unit: super::HELPERS + 1,
        set_unit: 5,
        realloc: super::HELPERS + 2,
    }
}

fn address(body: &mut Function, base: u32, offset: u32) {
    emit(
        body,
        &[
            Instruction::LocalGet(base),
            Instruction::I32Const(offset as i32),
            Instruction::I32Add,
        ],
    );
}
fn array_index(body: &mut Function, index: u32) {
    emit(
        body,
        &[Instruction::I32Const(index as i32), Instruction::RefI31],
    );
}
fn get(body: &mut Function, array: u32, index: u32) {
    body.instruction(&Instruction::LocalGet(array));
    array_index(body, index);
    body.instruction(&Instruction::Call(ARRAY_GET));
}
fn dynamic_get(body: &mut Function, array: u32, index: u32, wide: u32) {
    use Instruction::*;
    emit(body, &[LocalGet(array), LocalGet(index), I64ExtendI32U]);
    for instruction in crate::runtime_abi::long_box_in(wide) {
        body.instruction(&instruction);
    }
    body.instruction(&Call(ARRAY_GET));
}
fn array_length(body: &mut Function, value: u32, locals: &Locals) {
    emit(
        body,
        &[
            Instruction::LocalGet(value),
            Instruction::Call(ARRAY_LENGTH),
            Instruction::LocalSet(locals.value),
        ],
    );
    for instruction in crate::runtime_abi::long_to_i64(locals.value) {
        body.instruction(&instruction);
    }
    emit(
        body,
        &[
            Instruction::I32WrapI64,
            Instruction::LocalSet(locals.length),
        ],
    );
}

fn packed_data(body: &mut Function, value: u32) {
    use Instruction::*;
    emit(
        body,
        &[
            LocalGet(value),
            RefCastNonNull(HeapType::Concrete(crate::runtime_abi::PACKED)),
            StructGet {
                struct_type_index: crate::runtime_abi::PACKED,
                field_index: 0,
            },
        ],
    );
}
fn allocate_array(body: &mut Function, length: u32, locals: &Locals) {
    emit(
        body,
        &[
            Instruction::LocalGet(length),
            Instruction::ArrayNewDefault(crate::runtime_abi::ARGS),
            Instruction::Call(ARRAY_NEW),
            Instruction::LocalSet(locals.tree),
        ],
    );
}

fn set(body: &mut Function, array: u32, index: u32, value: u32) {
    body.instruction(&Instruction::LocalGet(array));
    array_index(body, index);
    emit(
        body,
        &[
            Instruction::LocalGet(value),
            Instruction::Call(ARRAY_SET),
            Instruction::Drop,
        ],
    );
}
fn begin_loop(body: &mut Function, locals: &Locals) {
    use Instruction::*;
    emit(
        body,
        &[
            I32Const(0),
            LocalSet(locals.index),
            Block(BlockType::Empty),
            Loop(BlockType::Empty),
            LocalGet(locals.index),
            LocalGet(locals.length),
            I32GeU,
            BrIf(1),
        ],
    );
}
fn end_loop(body: &mut Function, locals: &Locals) {
    use Instruction::*;
    emit(
        body,
        &[
            LocalGet(locals.index),
            I32Const(1),
            I32Add,
            LocalSet(locals.index),
            Br(0),
            End,
            End,
        ],
    );
}
fn element_address(body: &mut Function, locals: &Locals, stride: u32) {
    use Instruction::*;
    emit(
        body,
        &[
            LocalGet(locals.data),
            LocalGet(locals.index),
            I32Const(stride as i32),
            I32Mul,
            I32Add,
        ],
    );
}
fn load_list(body: &mut Function, base: u32, locals: &Locals) {
    use Instruction::*;
    emit(
        body,
        &[
            LocalGet(base),
            I32Load(memory(0, 2)),
            LocalSet(locals.data),
            LocalGet(base),
            I32Load(memory(4, 2)),
            LocalSet(locals.length),
        ],
    );
}
fn fail(body: &mut Function, locals: &Locals, name: &str) {
    super::boundary_error_message(
        body,
        locals.value,
        &format!("WIT {name} has an invalid canonical discriminant"),
    );
}

/// Invoke one privately captured constructor/projector with its owned data.
pub(super) fn schema_call(body: &mut Function, root: u32, node: usize, project: bool, value: u32) {
    use Instruction::*;
    body.instruction(&GlobalGet(root));
    array_index(body, 2 * node as u32 + u32::from(project));
    emit(
        body,
        &[
            Call(ARRAY_GET),
            LocalGet(value),
            ArrayNewFixed {
                array_type_index: crate::runtime_abi::ARGS,
                array_size: 1,
            },
            Call(2),
        ],
    );
}

impl Registry<'_> {
    pub fn emit(
        &self,
        types: &mut TypeSection,
        functions: &mut FunctionSection,
        code: &mut CodeSection,
        base: u32,
        root: u32,
        sentinels: u32,
    ) {
        let mut signatures = vec![];
        let mut blocks = vec![];
        for node in &self.nodes {
            let mut store_params = node.flat.clone();
            store_params.push(ValType::I32);
            let shapes = [
                (vec![ValType::I32], vec![super::VALUE]),
                (vec![super::VALUE], vec![]),
                (vec![super::VALUE, ValType::I32], vec![]),
                (vec![ValType::I32], vec![]),
                (store_params, vec![]),
                (vec![ValType::I32], node.flat.clone()),
            ];
            for (params, results) in shapes {
                let index = types.len() + crate::runtime_abi::TYPE_COUNT - 1;
                types.ty().function(params, results);
                signatures.push(index);
            }
            let index = types.len() + crate::runtime_abi::TYPE_COUNT - 1;
            let payload = if matches!(node.shape, Shape::Variant { .. }) {
                &node.flat[1..]
            } else {
                &[]
            };
            types.ty().function([], payload.iter().copied());
            blocks.push(index);
        }
        for (id, node) in self.nodes.iter().enumerate() {
            for operation in 0..WIDTH {
                functions.function(signatures[id * WIDTH as usize + operation as usize]);
                let parameters = match operation {
                    WRITE => 2,
                    STORE_FLAT => node.flat.len() as u32 + 1,
                    _ => 1,
                };
                let locals = Locals::new(parameters);
                let mut declarations = Locals::declarations();
                if operation == LOAD_FLAT {
                    if let Shape::Variant { cases, .. } = &node.shape {
                        for (_, child) in cases {
                            if let Some(child) = child {
                                declarations
                                    .extend(self.nodes[*child].flat.iter().map(|ty| (1, *ty)));
                            }
                        }
                    }
                }
                let mut body = Function::new(declarations);
                match operation {
                    COPY => self.copy(&mut body, id, base, root, sentinels, &locals),
                    VALIDATE => self.validate(&mut body, id, base, sentinels, &locals),
                    WRITE => self.write(&mut body, id, base, sentinels, &locals),
                    FREE => self.free(&mut body, id, base, &locals),
                    STORE_FLAT => self.store_flat(&mut body, id, base, &locals),
                    LOAD_FLAT => self.load_flat(&mut body, id, base, blocks[id], &locals),
                    _ => unreachable!(),
                }
                body.instruction(&Instruction::End);
                code.function(&body);
            }
        }
    }

    fn copy(
        &self,
        body: &mut Function,
        id: usize,
        base: u32,
        root: u32,
        sentinels: u32,
        locals: &Locals,
    ) {
        use Instruction::*;
        let node = &self.nodes[id];
        match &node.shape {
            Shape::Scalar(scalar) => {
                emit(body, &[LocalGet(0), load(*scalar)]);
                super::lift_scalar(body, scalar, sentinels, locals.wide);
                if node.symbols.is_some() {
                    body.instruction(&LocalSet(locals.value));
                    schema_call(body, root, id, false, locals.value);
                }
                return;
            }
            Shape::String => {
                emit(
                    body,
                    &[
                        LocalGet(0),
                        I32Load(memory(0, 2)),
                        LocalSet(locals.pointer),
                        LocalGet(0),
                        I32Load(memory(4, 2)),
                        LocalSet(locals.length),
                    ],
                );
                super::strings::copy_in(body, &locals.string(), &strings());
                body.instruction(&LocalGet(locals.value));
                return;
            }
            Shape::Product { fields, .. } => {
                emit(
                    body,
                    &[I32Const(fields.len() as i32), LocalSet(locals.length)],
                );
                allocate_array(body, locals.length, locals);
                for (index, (_, offset, child)) in fields.iter().enumerate() {
                    address(body, 0, *offset);
                    emit(
                        body,
                        &[Call(function(base, *child, COPY)), LocalSet(locals.value)],
                    );
                    set(body, locals.tree, index as u32, locals.value);
                }
            }
            Shape::Variant {
                tag,
                payload,
                cases,
            } => {
                emit(
                    body,
                    &[
                        LocalGet(0),
                        load(tag_scalar(*tag)),
                        LocalSet(locals.integer),
                        I32Const(2),
                        LocalSet(locals.length),
                    ],
                );
                allocate_array(body, locals.length, locals);
                emit(
                    body,
                    &[LocalGet(locals.integer), RefI31, LocalSet(locals.value)],
                );
                set(body, locals.tree, 0, locals.value);
                for (index, (_, child)) in cases.iter().enumerate() {
                    emit(
                        body,
                        &[
                            LocalGet(locals.integer),
                            I32Const(index as i32),
                            I32Eq,
                            If(BlockType::Empty),
                        ],
                    );
                    if let Some(child) = child {
                        address(body, 0, *payload);
                        body.instruction(&Call(function(base, *child, COPY)));
                    } else {
                        body.instruction(&crate::runtime_abi::sentinel_value(
                            crate::runtime_abi::NIL,
                        ));
                    }
                    body.instruction(&LocalSet(locals.value));
                    set(body, locals.tree, 1, locals.value);
                    schema_call(body, root, id, false, locals.tree);
                    body.instruction(&Return).instruction(&End);
                }
                fail(body, locals, &node.name);
                return;
            }
            Shape::List {
                element, stride, ..
            } => {
                load_list(body, 0, locals);
                allocate_array(body, locals.length, locals);
                begin_loop(body, locals);
                element_address(body, locals, *stride);
                emit(
                    body,
                    &[
                        Call(function(base, *element, COPY)),
                        LocalSet(locals.value),
                        LocalGet(locals.tree),
                        LocalGet(locals.index),
                        RefI31,
                        LocalGet(locals.value),
                        Call(ARRAY_SET),
                        Drop,
                    ],
                );
                end_loop(body, locals);
            }
        }
        schema_call(body, root, id, false, locals.tree);
    }

    fn validate(&self, body: &mut Function, id: usize, base: u32, sentinels: u32, locals: &Locals) {
        use Instruction::*;
        let node = &self.nodes[id];
        match &node.shape {
            Shape::Scalar(scalar) => {
                emit(body, &[LocalGet(0), LocalSet(locals.value)]);
                super::lower_scalar(
                    body,
                    *scalar,
                    locals.value,
                    locals.integer,
                    locals.wide,
                    &node.name,
                    sentinels,
                );
                body.instruction(&Drop);
            }
            Shape::String => {
                emit(body, &[LocalGet(0), LocalSet(locals.value)]);
                super::strings::validate(body, &locals.string(), &strings(), |body| {
                    super::boundary_error_message(
                        body,
                        locals.value,
                        "WIT composite contains an incompatible Unicode string",
                    )
                });
            }
            Shape::Product { fields, .. } => {
                if let Some((_, kind)) = node.fixed_element.and_then(|child| self.packed(child)) {
                    packed_data(body, 0);
                    emit(
                        body,
                        &[
                            RefTestNonNull(HeapType::Concrete(crate::runtime_abi::ARGS)),
                            I32Eqz,
                            If(BlockType::Empty),
                        ],
                    );
                    packed_data(body, 0);
                    emit(
                        body,
                        &[
                            RefTestNonNull(HeapType::Concrete(
                                crate::runtime_abi::PACKED_STORAGE + kind,
                            )),
                            I32Eqz,
                            If(BlockType::Empty),
                        ],
                    );
                    super::boundary_error_message(
                        body,
                        locals.value,
                        "WIT fixed-length list returned a primitive array of another element type",
                    );
                    body.instruction(&End);
                    array_length(body, 0, locals);
                    emit(
                        body,
                        &[
                            LocalGet(locals.length),
                            I32Const(fields.len() as i32),
                            I32Ne,
                            If(BlockType::Empty),
                        ],
                    );
                    fail(body, locals, &node.name);
                    emit(body, &[End, Return, End]);
                }
                for (index, (_, _, child)) in fields.iter().enumerate() {
                    get(body, 0, index as u32);
                    body.instruction(&Call(function(base, *child, VALIDATE)));
                }
            }
            Shape::Variant { cases, .. } => {
                self.tree_tag(body, locals);
                for (index, (_, child)) in cases.iter().enumerate() {
                    emit(
                        body,
                        &[
                            LocalGet(locals.integer),
                            I32Const(index as i32),
                            I32Eq,
                            If(BlockType::Empty),
                        ],
                    );
                    if let Some(child) = child {
                        get(body, 0, 1);
                        body.instruction(&Call(function(base, *child, VALIDATE)));
                    }
                    body.instruction(&Return).instruction(&End);
                }
                fail(body, locals, &node.name);
            }
            Shape::List { element, .. } => {
                if let Some((_, kind)) = self.packed(*element) {
                    emit(
                        body,
                        &[
                            LocalGet(0),
                            RefCastNonNull(HeapType::Concrete(crate::runtime_abi::PACKED)),
                            StructGet {
                                struct_type_index: crate::runtime_abi::PACKED,
                                field_index: 0,
                            },
                            RefTestNonNull(HeapType::Concrete(crate::runtime_abi::ARGS)),
                            I32Eqz,
                            If(BlockType::Empty),
                            LocalGet(0),
                            RefCastNonNull(HeapType::Concrete(crate::runtime_abi::PACKED)),
                            StructGet {
                                struct_type_index: crate::runtime_abi::PACKED,
                                field_index: 0,
                            },
                            RefTestNonNull(HeapType::Concrete(
                                crate::runtime_abi::PACKED_STORAGE + kind,
                            )),
                            I32Eqz,
                            If(BlockType::Empty),
                        ],
                    );
                    super::boundary_error_message(
                        body,
                        locals.value,
                        "WIT composite list returned a primitive array of another element type",
                    );
                    emit(body, &[End, Return, End]);
                }
                array_length(body, 0, locals);
                begin_loop(body, locals);
                dynamic_get(body, 0, locals.index, locals.wide);
                body.instruction(&Call(function(base, *element, VALIDATE)));
                end_loop(body, locals);
            }
        }
    }

    fn tree_tag(&self, body: &mut Function, locals: &Locals) {
        get(body, 0, 0);
        body.instruction(&Instruction::LocalSet(locals.value));
        for instruction in crate::runtime_abi::long_to_i64(locals.value) {
            body.instruction(&instruction);
        }
        emit(
            body,
            &[
                Instruction::I32WrapI64,
                Instruction::LocalSet(locals.integer),
            ],
        );
    }

    fn write(&self, body: &mut Function, id: usize, base: u32, sentinels: u32, locals: &Locals) {
        use Instruction::*;
        let node = &self.nodes[id];
        match &node.shape {
            Shape::Scalar(scalar) => {
                emit(body, &[LocalGet(0), LocalSet(locals.value), LocalGet(1)]);
                super::lower_scalar(
                    body,
                    *scalar,
                    locals.value,
                    locals.integer,
                    locals.wide,
                    &node.name,
                    sentinels,
                );
                body.instruction(&store(*scalar));
            }
            Shape::String => {
                emit(
                    body,
                    &[
                        LocalGet(0),
                        LocalSet(locals.value),
                        LocalGet(0),
                        Call(super::HELPERS),
                        LocalSet(locals.length),
                    ],
                );
                super::strings::copy_out(body, &locals.string(), &strings());
                emit(
                    body,
                    &[
                        LocalGet(1),
                        LocalGet(locals.pointer),
                        I32Store(memory(0, 2)),
                        LocalGet(1),
                        LocalGet(locals.length),
                        I32Store(memory(4, 2)),
                    ],
                );
            }
            Shape::Product { fields, .. } => {
                if let Some((scalar, kind)) =
                    node.fixed_element.and_then(|child| self.packed(child))
                {
                    let storage = crate::runtime_abi::PACKED_STORAGE + kind;
                    packed_data(body, 0);
                    emit(
                        body,
                        &[
                            RefTestNonNull(HeapType::Concrete(storage)),
                            If(BlockType::Empty),
                            LocalGet(1),
                            LocalSet(locals.data),
                            I32Const(fields.len() as i32),
                            LocalSet(locals.length),
                        ],
                    );
                    begin_loop(body, locals);
                    element_address(body, locals, self.nodes[node.fixed_element.unwrap()].size);
                    packed_data(body, 0);
                    emit(
                        body,
                        &[
                            RefCastNonNull(HeapType::Concrete(storage)),
                            LocalGet(locals.index),
                            if kind < 2 {
                                ArrayGetU(storage)
                            } else {
                                ArrayGet(storage)
                            },
                            store(scalar),
                        ],
                    );
                    end_loop(body, locals);
                    emit(body, &[Return, End]);
                }
                for (index, (_, offset, child)) in fields.iter().enumerate() {
                    get(body, 0, index as u32);
                    address(body, 1, *offset);
                    body.instruction(&Call(function(base, *child, WRITE)));
                }
            }
            Shape::Variant {
                tag,
                payload,
                cases,
            } => {
                self.tree_tag(body, locals);
                emit(
                    body,
                    &[
                        LocalGet(1),
                        LocalGet(locals.integer),
                        store(tag_scalar(*tag)),
                    ],
                );
                for (index, (_, child)) in cases.iter().enumerate() {
                    emit(
                        body,
                        &[
                            LocalGet(locals.integer),
                            I32Const(index as i32),
                            I32Eq,
                            If(BlockType::Empty),
                        ],
                    );
                    if let Some(child) = child {
                        get(body, 0, 1);
                        address(body, 1, *payload);
                        body.instruction(&Call(function(base, *child, WRITE)));
                    }
                    body.instruction(&Return).instruction(&End);
                }
                fail(body, locals, &node.name);
            }
            Shape::List {
                element, stride, ..
            } => {
                array_length(body, 0, locals);
                emit(
                    body,
                    &[
                        I32Const(0),
                        I32Const(0),
                        I32Const(self.nodes[*element].align as i32),
                        LocalGet(locals.length),
                        I64ExtendI32U,
                        I64Const(i64::from(*stride)),
                        I64Mul,
                        LocalTee(locals.wide),
                        I64Const(i64::from(u32::MAX)),
                        I64GtU,
                        If(BlockType::Empty),
                        Unreachable,
                        End,
                        LocalGet(locals.wide),
                        I32WrapI64,
                        Call(super::HELPERS + 2),
                        LocalSet(locals.data),
                        LocalGet(1),
                        LocalGet(locals.data),
                        I32Store(memory(0, 2)),
                        LocalGet(1),
                        LocalGet(locals.length),
                        I32Store(memory(4, 2)),
                    ],
                );
                if let Some((scalar, kind)) = self.packed(*element) {
                    emit(
                        body,
                        &[
                            LocalGet(0),
                            RefCastNonNull(HeapType::Concrete(crate::runtime_abi::PACKED)),
                            StructGet {
                                struct_type_index: crate::runtime_abi::PACKED,
                                field_index: 0,
                            },
                            RefTestNonNull(HeapType::Concrete(
                                crate::runtime_abi::PACKED_STORAGE + kind,
                            )),
                            If(BlockType::Empty),
                        ],
                    );
                    let storage = crate::runtime_abi::PACKED_STORAGE + kind;
                    begin_loop(body, locals);
                    element_address(body, locals, *stride);
                    emit(
                        body,
                        &[
                            LocalGet(0),
                            RefCastNonNull(HeapType::Concrete(crate::runtime_abi::PACKED)),
                            StructGet {
                                struct_type_index: crate::runtime_abi::PACKED,
                                field_index: 0,
                            },
                            RefCastNonNull(HeapType::Concrete(storage)),
                            LocalGet(locals.index),
                            if kind < 2 {
                                ArrayGetU(storage)
                            } else {
                                ArrayGet(storage)
                            },
                            store(scalar),
                        ],
                    );
                    end_loop(body, locals);
                    emit(body, &[Return, End]);
                }
                begin_loop(body, locals);
                dynamic_get(body, 0, locals.index, locals.wide);
                element_address(body, locals, *stride);
                body.instruction(&Call(function(base, *element, WRITE)));
                end_loop(body, locals);
            }
        }
    }

    fn free(&self, body: &mut Function, id: usize, base: u32, locals: &Locals) {
        use Instruction::*;
        match &self.nodes[id].shape {
            Shape::Scalar(_) => {}
            Shape::String => emit(
                body,
                &[
                    LocalGet(0),
                    I32Load(memory(0, 2)),
                    LocalGet(0),
                    I32Load(memory(4, 2)),
                    I32Const(2),
                    I32Mul,
                    Call(super::HELPERS + 3),
                ],
            ),
            Shape::Product { fields, .. } => {
                for (_, offset, child) in fields {
                    if self.nodes[*child].dynamic {
                        address(body, 0, *offset);
                        body.instruction(&Call(function(base, *child, FREE)));
                    }
                }
            }
            Shape::Variant {
                tag,
                payload,
                cases,
            } => {
                emit(
                    body,
                    &[
                        LocalGet(0),
                        load(tag_scalar(*tag)),
                        LocalSet(locals.integer),
                    ],
                );
                for (index, (_, child)) in cases.iter().enumerate() {
                    emit(
                        body,
                        &[
                            LocalGet(locals.integer),
                            I32Const(index as i32),
                            I32Eq,
                            If(BlockType::Empty),
                        ],
                    );
                    if let Some(child) = child {
                        if self.nodes[*child].dynamic {
                            address(body, 0, *payload);
                            body.instruction(&Call(function(base, *child, FREE)));
                        }
                    }
                    body.instruction(&Return).instruction(&End);
                }
                fail(body, locals, &self.nodes[id].name);
            }
            Shape::List {
                element, stride, ..
            } => {
                load_list(body, 0, locals);
                if self.nodes[*element].dynamic {
                    begin_loop(body, locals);
                    element_address(body, locals, *stride);
                    body.instruction(&Call(function(base, *element, FREE)));
                    end_loop(body, locals);
                }
                emit(
                    body,
                    &[
                        LocalGet(locals.data),
                        LocalGet(locals.length),
                        I32Const(*stride as i32),
                        I32Mul,
                        Call(super::HELPERS + 3),
                    ],
                );
            }
        }
    }

    fn store_flat(&self, body: &mut Function, id: usize, base: u32, locals: &Locals) {
        use Instruction::*;
        let node = &self.nodes[id];
        let pointer = node.flat.len() as u32;
        match &node.shape {
            Shape::Scalar(scalar) => {
                if let Some((flags, names)) = &node.symbols {
                    if !flags || names.len() < 32 {
                        emit(
                            body,
                            &[
                                LocalGet(0),
                                I32Const(if *flags {
                                    (1_u32 << names.len()) as i32
                                } else {
                                    names.len() as i32
                                }),
                                I32GeU,
                                If(BlockType::Empty),
                            ],
                        );
                        fail(body, locals, &node.name);
                        body.instruction(&End);
                    }
                }
                emit(body, &[LocalGet(pointer), LocalGet(0), store(*scalar)]);
            }
            Shape::String | Shape::List { .. } => emit(
                body,
                &[
                    LocalGet(pointer),
                    LocalGet(0),
                    I32Store(memory(0, 2)),
                    LocalGet(pointer),
                    LocalGet(1),
                    I32Store(memory(4, 2)),
                ],
            ),
            Shape::Product { fields, .. } => {
                let mut field = 0;
                for (_, offset, child) in fields {
                    for index in 0..self.nodes[*child].flat.len() {
                        body.instruction(&LocalGet(field + index as u32));
                    }
                    address(body, pointer, *offset);
                    body.instruction(&Call(function(base, *child, STORE_FLAT)));
                    field += self.nodes[*child].flat.len() as u32;
                }
            }
            Shape::Variant {
                tag,
                payload,
                cases,
            } => {
                emit(
                    body,
                    &[LocalGet(pointer), LocalGet(0), store(tag_scalar(*tag))],
                );
                for (index, (_, child)) in cases.iter().enumerate() {
                    emit(
                        body,
                        &[
                            LocalGet(0),
                            I32Const(index as i32),
                            I32Eq,
                            If(BlockType::Empty),
                        ],
                    );
                    if let Some(child) = child {
                        for (field, ty) in self.nodes[*child].flat.iter().enumerate() {
                            body.instruction(&LocalGet(field as u32 + 1));
                            cast(body, node.flat[field + 1], *ty);
                        }
                        address(body, pointer, *payload);
                        body.instruction(&Call(function(base, *child, STORE_FLAT)));
                    }
                    body.instruction(&Return).instruction(&End);
                }
                fail(body, locals, &node.name);
            }
        }
    }

    fn load_flat(&self, body: &mut Function, id: usize, base: u32, block: u32, locals: &Locals) {
        use Instruction::*;
        let node = &self.nodes[id];
        match &node.shape {
            Shape::Scalar(scalar) => emit(body, &[LocalGet(0), load(*scalar)]),
            Shape::String | Shape::List { .. } => emit(
                body,
                &[
                    LocalGet(0),
                    I32Load(memory(0, 2)),
                    LocalGet(0),
                    I32Load(memory(4, 2)),
                ],
            ),
            Shape::Product { fields, .. } => {
                for (_, offset, child) in fields {
                    address(body, 0, *offset);
                    body.instruction(&Call(function(base, *child, LOAD_FLAT)));
                }
            }
            Shape::Variant {
                tag,
                payload,
                cases,
            } => {
                emit(
                    body,
                    &[
                        LocalGet(0),
                        load(tag_scalar(*tag)),
                        LocalTee(locals.integer),
                    ],
                );
                let mut temporary = locals.data + 1;
                for (index, (_, child)) in cases.iter().enumerate() {
                    emit(
                        body,
                        &[
                            LocalGet(locals.integer),
                            I32Const(index as i32),
                            I32Eq,
                            If(BlockType::FunctionType(block)),
                        ],
                    );
                    if let Some(child) = child {
                        address(body, 0, *payload);
                        body.instruction(&Call(function(base, *child, LOAD_FLAT)));
                        let values = &self.nodes[*child].flat;
                        for field in (0..values.len()).rev() {
                            body.instruction(&LocalSet(temporary + field as u32));
                        }
                        for (field, ty) in node.flat[1..].iter().enumerate() {
                            if field < values.len() {
                                body.instruction(&LocalGet(temporary + field as u32));
                                cast(body, values[field], *ty);
                            } else {
                                zero(body, *ty);
                            }
                        }
                        temporary += values.len() as u32;
                    } else {
                        for ty in &node.flat[1..] {
                            zero(body, *ty);
                        }
                    }
                    body.instruction(&Else);
                }
                fail(body, locals, &node.name);
                for _ in cases {
                    body.instruction(&End);
                }
            }
        }
    }
}

fn zero(body: &mut Function, ty: ValType) {
    body.instruction(&match ty {
        ValType::I32 => Instruction::I32Const(0),
        ValType::I64 => Instruction::I64Const(0),
        ValType::F32 => Instruction::F32Const(0.0.into()),
        ValType::F64 => Instruction::F64Const(0.0.into()),
        _ => unreachable!(),
    });
}
/// Canonical variant joins carry float payload bits, never numeric conversions.
fn cast(body: &mut Function, from: ValType, to: ValType) {
    use Instruction::*;
    if from == to {
        return;
    }
    match (from, to) {
        (ValType::I32, ValType::F32) => {
            body.instruction(&F32ReinterpretI32);
        }
        (ValType::F32, ValType::I32) => {
            body.instruction(&I32ReinterpretF32);
        }
        (ValType::I64, ValType::F64) => {
            body.instruction(&F64ReinterpretI64);
        }
        (ValType::F64, ValType::I64) => {
            body.instruction(&I64ReinterpretF64);
        }
        (ValType::I64, ValType::I32) => {
            body.instruction(&I32WrapI64);
        }
        (ValType::I32, ValType::I64) => {
            body.instruction(&I64ExtendI32U);
        }
        (ValType::I64, ValType::F32) => {
            emit(body, &[I32WrapI64, F32ReinterpretI32]);
        }
        (ValType::F32, ValType::I64) => {
            emit(body, &[I32ReinterpretF32, I64ExtendI32U]);
        }
        _ => unreachable!("invalid canonical join {from:?} -> {to:?}"),
    }
}
