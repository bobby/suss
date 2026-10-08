//! Analysis/context prerequisites only. No source future executes in this suite.
use suss_compile::portable::{
    self,
    hir::{BindingId, Expression, Hir, Literal, SourceNode, Type},
    ir,
    resolve::{Environment, Phase},
};

fn analyze(source: &str) -> Hir {
    portable::analyze(source).unwrap()
}
fn single(mut hir: &Hir) -> &Hir {
    while let Expression::Do(items) = &hir.kind {
        assert_eq!(items.len(), 1);
        hir = &items[0];
    }
    hir
}
fn number(hir: &Hir, expected: f64) {
    assert!(
        matches!(&hir.kind, Expression::Literal(Literal::Number(n)) if n.to_bits() == expected.to_bits())
    );
}
fn await_local(hir: &Hir, expected: BindingId) {
    let Expression::Await { value } = &single(hir).kind else {
        panic!("await")
    };
    assert!(matches!(&value.kind, Expression::Local(id) if *id == expected));
}

#[test]
fn future_captures_outer_locals_but_not_its_sequential_bindings() {
    let hir = analyze(
        "(let [x 10 y 20] (suss.async/future* (let [a (suss.async/await* x) b (suss.async/await* a)] y b)))",
    );
    let Expression::Let { bindings, body } = &single(&hir).kind else {
        panic!("outer let")
    };
    let x = bindings[0].id;
    let y = bindings[1].id;
    let future = single(body);
    assert_eq!(future.ty, Type::Value);
    let Expression::Future { captures, body } = &future.kind else {
        panic!("future")
    };
    assert_eq!(captures, &[x, y]);
    assert!(matches!(
        future.source.as_ref().unwrap().node.as_deref(),
        Some(SourceNode::Future(_))
    ));
    let Expression::Let { bindings, body } = &single(body).kind else {
        panic!("inner let")
    };
    await_local(&bindings[0].value, x);
    await_local(&bindings[1].value, bindings[0].id);
    let Expression::Do(items) = &body.kind else {
        panic!("body")
    };
    assert!(matches!(&items[0].kind, Expression::Local(id) if *id == y));
    assert!(matches!(&items[1].kind, Expression::Local(id) if *id == bindings[1].id));
    assert!(!captures.contains(&bindings[0].id));
    assert!(!captures.contains(&bindings[1].id));
}

#[test]
fn async_try_and_dynamic_regions_retain_executable_bodies_and_handler_scope() {
    let hir = analyze(
        "(def ^:dynamic *v* 0) (let [x 7] (suss.async/future* (try (binding [*v* (suss.async/await* x)] (suss.async/await* x)) (catch :default e (suss.async/await* e)) (finally (suss.async/await* x)))))",
    );
    let Expression::Do(top) = &hir.kind else {
        panic!("top")
    };
    let Expression::Let { bindings, body } = &top[1].kind else {
        panic!("let")
    };
    let x = bindings[0].id;
    let Expression::Future { captures, body } = &single(body).kind else {
        panic!("future")
    };
    assert_eq!(captures, &[x]);
    let region = single(body);
    let Expression::AsyncTry {
        body,
        handler,
        cleanup,
        payload,
    } = &region.kind
    else {
        panic!("async try")
    };
    assert!(matches!(
        region.source.as_ref().unwrap().node.as_deref(),
        Some(SourceNode::Try { .. })
    ));
    let Expression::AsyncDynamicScope { bindings, body } = &single(body).kind else {
        panic!("async dynamic")
    };
    await_local(&bindings[0].1, x);
    await_local(body, x);
    let payload = payload.as_ref().unwrap();
    let Expression::Let {
        bindings,
        body: handler_body,
    } = &handler.as_ref().unwrap().kind
    else {
        panic!("catch alias let")
    };
    assert!(
        bindings.is_empty(),
        "catch alias uses the bound payload directly"
    );
    await_local(handler_body, payload.id);
    await_local(cleanup.as_ref().unwrap(), x);
    assert!(!captures.contains(&payload.id));
}

#[test]
fn await_context_resets_at_ordinary_functions_and_restores_after_nested_futures() {
    for source in [
        "(suss.async/await* 1)",
        "(suss.async/future* (fn* [] (suss.async/await* 1)))",
        "(suss.async/future* (fn* ([x] (suss.async/await* x)) ([x y] y)))",
        "(suss.async/future* (try (fn* [] (suss.async/await* 1))))",
        "(suss.async/future* 1) (suss.async/await* 2)",
        "(let [x 1] (suss.async/future* (fn* [] (suss.async/future* (suss.async/await* x)) (suss.async/await* x))))",
    ] {
        let error = portable::analyze(source).unwrap_err();
        assert!(
            error.message.contains("await")
                && error.message.contains("inside")
                && error.message.contains("future"),
            "{error}"
        );
        assert!(error.span.start < error.span.end && error.span.end <= source.len());
    }
    for source in [
        "(suss.async/future* (fn* [] 1) (suss.async/await* 2))",
        "(suss.async/future* (suss.async/future* (suss.async/await* 1)) (suss.async/await* 2))",
        "(fn* [x] (suss.async/future* (suss.async/await* x)))",
    ] {
        analyze(source);
    }
    for source in [
        "(suss.async/future* (suss.async/await*))",
        "(suss.async/future* (suss.async/await* 1 2))",
    ] {
        assert!(
            portable::analyze(source)
                .unwrap_err()
                .message
                .contains("exactly one")
        );
    }
}

#[test]
fn async_body_order_and_provisional_global_identity_are_retained_without_publication() {
    let hir = analyze(
        "(suss.async/future* 17 (suss.async/await* (do 41 42)) 99 (def deferred 3) deferred)",
    );
    let Expression::Future { captures, body } = &single(&hir).kind else {
        panic!("future")
    };
    assert!(captures.is_empty());
    let Expression::Do(items) = &body.kind else {
        panic!("body")
    };
    assert_eq!(items.len(), 5);
    number(&items[0], 17.0);
    let Expression::Await { value } = &items[1].kind else {
        panic!("await")
    };
    let Expression::Do(operands) = &value.kind else {
        panic!("operand do")
    };
    assert_eq!(operands.len(), 2);
    number(&operands[0], 41.0);
    number(&operands[1], 42.0);
    assert!(operands[0].span.start < operands[1].span.start);
    number(&items[2], 99.0);
    let Expression::Definition {
        global,
        initializer,
        ..
    } = &items[3].kind
    else {
        panic!("def")
    };
    number(initializer.as_ref().unwrap(), 3.0);
    assert!(matches!(&items[4].kind, Expression::Global(reference) if reference == global));
    for pair in items.windows(2) {
        assert!(pair[0].span.start < pair[1].span.start);
    }
    let environment = Environment::default();
    let prepared = portable::prepare_fragment(
        "(suss.async/future* (def deferred 3) deferred)",
        &environment,
        Phase::Runtime,
    )
    .expect("plain future creation emits a compiled continuation");
    assert!(!prepared.wasm.is_empty());
    assert!(portable::analyze_in("deferred", &environment, Phase::Runtime).is_err());
}

#[test]
fn synchronous_regions_keep_existing_closure_shapes_and_plain_async_lowering_is_explicit() {
    let hir = analyze("(try 1 (catch :default e e) (finally 2))");
    let Expression::Try { regions } = &single(&hir).kind else {
        panic!("sync try")
    };
    for (region, arity) in regions.iter().zip([0, 1, 0]) {
        assert!(
            matches!(&region.kind, Expression::Function { parameters, .. } if parameters.len() == arity)
        );
    }
    ir::lower(&hir).unwrap();
    let hir = analyze("(def ^:dynamic *v* 1) (binding [*v* 2] *v*)");
    let Expression::Do(top) = &hir.kind else {
        panic!("top")
    };
    let Expression::DynamicScope { body, .. } = &top[1].kind else {
        panic!("sync dynamic")
    };
    assert!(matches!(&body.kind, Expression::Function { parameters, .. } if parameters.is_empty()));
    ir::lower(&hir).unwrap();
    for source in [
        "(suss.async/future*)",
        "(suss.async/future* 42)",
        "(suss.async/future* (suss.async/await* 42))",
    ] {
        let hir = analyze(source);
        let lowered = ir::lower(&hir).unwrap();
        ir::verify(&lowered).unwrap();
    }
    for (source, dynamic) in [
        ("(suss.async/future* (try 1 (finally 2)))", false),
        (
            "(def ^:dynamic *x* 1) (suss.async/future* (binding [*x* 2] *x*))",
            true,
        ),
    ] {
        let lowered = ir::lower(&analyze(source)).unwrap();
        ir::verify(&lowered).unwrap();
        let body = lowered
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .find_map(|inst| {
                if let ir::Operation::MakeFuture { body, .. } = &inst.operation {
                    Some(body)
                } else {
                    None
                }
            })
            .expect("compiled future body");
        let operations: Vec<_> = body
            .continuation
            .function
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .map(|inst| &inst.operation)
            .collect();
        assert!(
            operations
                .iter()
                .any(|op| matches!(op, ir::Operation::RegionPush { .. }))
        );
        assert!(
            operations
                .iter()
                .any(|op| matches!(op, ir::Operation::RegionExit { .. }))
        );
        assert!(
            operations
                .iter()
                .any(|op| matches!(op, ir::Operation::RegionValue))
        );
        assert_eq!(
            operations
                .iter()
                .any(|op| matches!(op, ir::Operation::AsyncDynamicEnter { .. })),
            dynamic
        );
        let artifact = portable::compile_ir(&lowered).unwrap();
        assert!(!artifact.is_empty());
    }
    // The private markers are qualified compiler forms, not bare-name rewrites.
    let hir = analyze("(let [future* (fn* [x] x) await* (fn* [x] x)] (future* (await* 42)))");
    let Expression::Let { body, .. } = &single(&hir).kind else {
        panic!("let")
    };
    assert!(matches!(&single(body).kind, Expression::Call { .. }));
}

#[test]
fn future_cannot_recur_to_enclosing_loop_but_can_contain_its_own_loop() {
    let error = portable::analyze("(loop* [x 1] (suss.async/future* (recur x)))").unwrap_err();
    assert!(error.message.contains("recur requires an enclosing"));
    let hir = analyze("(loop* [x 1] (suss.async/future* (loop* [y x] (recur y))) (recur x))");
    let Expression::Loop { body, .. } = &single(&hir).kind else {
        panic!("outer loop")
    };
    let Expression::Do(items) = &body.kind else {
        panic!("body")
    };
    assert!(matches!(&items[0].kind, Expression::Future { .. }));
    assert!(matches!(&items[1].kind, Expression::Recur { .. }));
}

#[test]
fn enclosing_closure_captures_values_needed_only_by_its_nested_future() {
    let hir = analyze("(let [x 1] (fn* [] (suss.async/future* (suss.async/await* x))))");
    let Expression::Let { bindings, body } = &single(&hir).kind else {
        panic!("let")
    };
    let x = bindings[0].id;
    let Expression::Function { captures, body, .. } = &single(body).kind else {
        panic!("fn")
    };
    assert_eq!(captures, &[x]);
    let Expression::Loop { body, .. } = &body.kind else {
        panic!("function recurrence wrapper")
    };
    let Expression::Future { captures, body } = &single(body).kind else {
        panic!("future")
    };
    assert_eq!(captures, &[x]);
    await_local(body, x);
}

#[test]
fn object_methods_reset_await_context_but_nested_futures_and_caller_await_are_legal() {
    for (prefix, method) in [
        (
            "",
            "(deftype ObjectAsyncBoundary [] Object (valueOf [this] BODY))",
        ),
        // Native number extensions take the declared-protocol path, where
        // Object is not a protocol binding. A real nominal target reaches the
        // Object method boundary instead; no core-library loading is assumed.
        (
            "(deftype ObjectAsyncTarget [])",
            "(extend-type ObjectAsyncTarget Object (valueOf [this] BODY))",
        ),
    ] {
        let source = format!(
            "{prefix} (suss.async/future* {})",
            method.replace("BODY", "(suss.async/await* this)")
        );
        let error = portable::analyze(&source).unwrap_err();
        assert!(
            error.message.contains("await")
                && error.message.contains("inside")
                && error.message.contains("future"),
            "{source}: {error}"
        );
        assert!(error.span.start < error.span.end && error.span.end <= source.len());
        // A nested future establishes its own context. The outer await also
        // proves the method boundary restored its caller's suspendable context.
        let source = format!(
            "{prefix} (suss.async/future* {} (suss.async/await* 42))",
            method.replace("BODY", "(suss.async/future* (suss.async/await* this))")
        );
        analyze(&source);
    }
}

#[test]
fn protocol_methods_reset_await_context_but_nested_futures_and_caller_await_are_legal() {
    for method in [
        "(deftype ProtocolAsyncBoundary [] AsyncBoundary (read-boundary [this] BODY))",
        "(extend-type number AsyncBoundary (read-boundary [this] BODY))",
    ] {
        let prefix = "(defprotocol AsyncBoundary (read-boundary [this]))";
        let source = format!(
            "{prefix} (suss.async/future* {})",
            method.replace("BODY", "(suss.async/await* this)")
        );
        let error = portable::analyze(&source).unwrap_err();
        assert!(
            error.message.contains("await")
                && error.message.contains("inside")
                && error.message.contains("future"),
            "{source}: {error}"
        );
        assert!(error.span.start < error.span.end && error.span.end <= source.len());
        let source = format!(
            "{prefix} (suss.async/future* {} (suss.async/await* 42))",
            method.replace("BODY", "(suss.async/future* (suss.async/await* this))")
        );
        analyze(&source);
    }
}

#[test]
fn ordinary_object_and_protocol_methods_do_not_inherit_future_await_context() {
    for (prefix, implementation) in [
        (
            "",
            "(deftype MethodObject [] Object (toString [this] BODY))",
        ),
        (
            "(defprotocol MethodProtocol (read-it [this]))",
            "(deftype MethodImpl [] MethodProtocol (read-it [this] BODY))",
        ),
    ] {
        let direct = format!(
            "{prefix} (suss.async/future* {})",
            implementation.replace("BODY", "(suss.async/await* 42)")
        );
        let error = portable::analyze(&direct)
            .expect_err("ordinary methods cannot await their enclosing future");
        assert!(
            error.message.contains("await")
                && error.message.contains("inside")
                && error.message.contains("future"),
            "{error}"
        );
        assert!(error.span.start < error.span.end);
        let nested = format!(
            "{prefix} (suss.async/future* {} (suss.async/await* 17))",
            implementation.replace("BODY", "(suss.async/future* (suss.async/await* 42))")
        );
        portable::analyze(&nested).expect(
            "a method's nested future has its own await context and restores the outer context",
        );
    }
}
