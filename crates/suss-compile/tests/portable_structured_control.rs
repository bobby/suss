//! Reducible control flow compiles to `block`, `loop` and `br`, without the
//! block-index dispatcher (#358).
use suss_compile::portable::{
    self,
    resolve::{Environment, Phase},
};

/// For each function body: its `loop` count, and whether it touches the
/// dispatcher's block index. That is the first declared `i32` local; raw
/// truth values from unboxed comparisons (#337) are `i32` locals after it.
fn functions(source: &str) -> Vec<(usize, bool)> {
    use wasmparser::{CompositeInnerType, Operator, Parser, Payload, ValType};
    let wasm = portable::compile_in(source, &Environment::default(), Phase::Runtime).unwrap();
    let (mut types, mut parameters, mut kinds, mut found) = (vec![], vec![], vec![], vec![]);
    for payload in Parser::new(0).parse_all(&wasm) {
        let body = match payload.unwrap() {
            Payload::TypeSection(section) => {
                for group in section {
                    types.extend(group.unwrap().types().map(|ty| match &ty.composite_type.inner {
                        CompositeInnerType::Func(ty) => Some(ty.params().len() as u32),
                        _ => None,
                    }));
                }
                continue;
            }
            Payload::FunctionSection(section) => {
                for ty in section {
                    let ty = ty.unwrap();
                    parameters.push(types[ty as usize]);
                    kinds.push(ty);
                }
                continue;
            }
            // A spread entry (ADR-0004) counts its arguments in an i32
            // local; it has no control flow to dispatch.
            Payload::CodeSectionEntry(_) if kinds[found.len()] == suss_compile::runtime_abi::SPREAD => {
                found.push((0, false));
                continue;
            }
            Payload::CodeSectionEntry(body) => body,
            _ => continue,
        };
        let mut index = parameters[found.len()].expect("function type");
        let mut dispatch = None;
        for local in body.get_locals_reader().unwrap() {
            let (count, ty) = local.unwrap();
            if ty == ValType::I32 && dispatch.is_none() { dispatch = Some(index); }
            index += count;
        }
        let (mut loops, mut touched) = (0, false);
        for operator in body.get_operators_reader().unwrap() {
            match operator.unwrap() {
                Operator::Loop { .. } => loops += 1,
                Operator::LocalGet { local_index } | Operator::LocalSet { local_index } => {
                    touched |= dispatch == Some(local_index);
                }
                _ => {}
            }
        }
        found.push((loops, touched));
    }
    found
}

#[test]
fn loops_and_ifs_compile_without_block_dispatch() {
    for source in [
        "(fn [x] x)",
        "(fn [n g] (loop [i 0 c 0] (if (< i n) (recur (inc i) (g i)) c)))",
        "(fn [x] (if x (if (nil? x) 1 2) (let [y (if (identical? x 3) 4 5)] y)))",
        "(fn [n] (loop [i 0] (if (< i n) (recur (loop [j i] (if (< j 3) (recur (inc j)) (inc j)))) i)))",
    ] {
        let functions = functions(source);
        assert!(functions.iter().all(|(_, touched)| !touched), "{source} dispatches: {functions:?}");
    }
    let loops = |source| functions(source).iter().map(|(loops, _)| loops).sum::<usize>();
    assert_eq!(loops("(fn [n g] (loop [i 0 c 0] (if (< i n) (recur (inc i) (g i)) c)))"), 1);
    assert_eq!(loops("(fn [x] (if x 1 2))"), 0);
}
