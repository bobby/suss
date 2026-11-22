//! Literal-vector scalar storage preserves the generic lookup boundary (#351).
use suss_compile::portable::{
    self,
    hir::Nominal,
    ir::{self, Operation},
    resolve::{Environment, Phase},
};

fn lower(source: &str) -> ir::Function {
    let mut env = Environment::default();
    for name in ["PersistentVector", "nth"] {
        env.declare_cell(Phase::Runtime, "suss.core", name).unwrap();
    }
    env.set_direct_linking(true);
    let hir = portable::analyze_in(source, &env, Phase::Runtime).unwrap();
    let graph = ir::lower(&hir).unwrap();
    ir::verify(&graph).unwrap();
    portable::compile_ir(&graph).unwrap();
    graph
}
fn guards(graph: &ir::Function) -> usize {
    graph
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .map(|inst| match &inst.operation {
            Operation::Nominal {
                operation: Nominal::ScalarVectorGuard,
                ..
            } => 1,
            Operation::MakeClosure { body, .. } => guards(&body.function),
            Operation::MakeGeneralClosure { body, .. } => {
                body.methods.iter().map(|m| guards(&m.function)).sum()
            }
            _ => 0,
        })
        .sum()
}

#[test]
fn known_vector_projections_get_a_guarded_scalar_continuation() {
    for source in [
        "(let [v [3000000000 1.5] a (nth v 0 nil) b (nth v 1 nil)] (+ a b))",
        "(fn [] (let [v [3000000000 1.5] a (nth v 0 nil)] a))",
        "(loop [i 0 acc 0.0] (if (< i 4) (let [v [3000000000 1.5] a (nth v 0 nil) b (nth v 1 nil)] (recur (+ i 1) (+ acc (+ a b)))) acc))",
    ] {
        assert_eq!(guards(&lower(source)), 1, "{source}");
    }
}

#[test]
fn escaping_or_unknown_vector_projections_retain_materialization() {
    for source in [
        "(let [v [3000000000 1.5]] v)",
        "(fn [i] (let [v [3000000000 1.5]] (nth v i nil)))",
        "(let [v [3000000000 1.5]] (nth v 2 nil))",
        "(let [v [3000000000 1.5] a (nth v 0 nil)] (if a v nil))",
    ] {
        assert_eq!(guards(&lower(source)), 0, "{source}");
    }
}
