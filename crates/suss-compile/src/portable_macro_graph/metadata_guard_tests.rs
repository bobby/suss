//! Retained pre-change owned normalizer is an independent guard/order oracle.
use super::*;

fn owned_guard_oracle(form: &Form, charge: &mut impl FnMut(u64) -> Result<()>,
    visit: &mut impl FnMut(Option<&Form>)) -> Result<()> {
    let mut pending = vec![form];
    while let Some(form) = pending.pop() {
        visit(Some(form));
        let text_units = match &form.kind {
            Kind::String(value) => value.len(),
            Kind::Symbol(value) => value.to_string().encode_utf16().count(),
            Kind::Keyword(value) => {
                value.name.encode_utf16().count()
                    + value
                        .namespace
                        .as_ref()
                        .map_or(0, |ns| ns.encode_utf16().count() + 1)
            }
            _ => 0,
        };
        charge(1u64.saturating_add(text_units as u64))?;
        match &form.kind {
            Kind::List(items) | Kind::Vector(items) | Kind::Map(items) | Kind::Set(items) => {
                if matches!(form.kind, Kind::Map(_)) && items.len() % 2 != 0 {
                    return Err(SessionError::Host(wasmtime::Error::msg(
                        "Macro form map requires paired entries",
                    )));
                }
                pending.extend(items.iter().rev());
            }
            Kind::Conditional(_)
            | Kind::SplicingConditional(_)
            | Kind::Discard(_)
            | Kind::Prefix { .. } => {
                return Err(SessionError::Host(wasmtime::Error::msg(
                    "Native macro data requires resolved reader prefixes",
                )));
            }
            _ => {}
        }
        if !form.metadata.is_empty() {
            if !matches!(
                form.kind,
                Kind::Symbol(_) | Kind::List(_) | Kind::Vector(_) | Kind::Map(_) | Kind::Set(_)
            ) {
                return Err(SessionError::Host(wasmtime::Error::msg(
                    "Metadata requires a symbol or collection",
                )));
            }
            // Normalize and clone metadata within the same graph work budget.
            charge(form.metadata.len() as u64)?;
            let pairs = preserve_source_error(form.span.clone(), charge, |charge| {
                hir::reader_metadata_pairs_metered(form, &mut |work| charge(work))
            })?;
            let normalized = Form {
                span: form.span.clone(),
                metadata: vec![],
                kind: Kind::Map(pairs),
            };
            // Keep the temporary alive until it is visited below by validating
            // it immediately with a second iterative stack.
            let mut metadata_pending = vec![&normalized];
            while let Some(item) = metadata_pending.pop() {
                visit(if std::ptr::eq(item, &normalized) { None } else { Some(item) });
                let used = match &item.kind {
                    Kind::String(value) => value.len(),
                    Kind::Symbol(value) => value.to_string().encode_utf16().count(),
                    Kind::Keyword(value) => {
                        value.name.encode_utf16().count()
                            + value
                                .namespace
                                .as_ref()
                                .map_or(0, |ns| ns.encode_utf16().count() + 1)
                    }
                    _ => 0,
                };
                charge(1u64.saturating_add(used as u64))?;
                match &item.kind {
                    Kind::Map(items) if items.len() % 2 == 0 => {
                        metadata_pending.extend(items.iter().rev())
                    }
                    Kind::Map(_) => {
                        return Err(SessionError::Host(wasmtime::Error::msg(
                            "Macro form map requires paired entries",
                        )));
                    }
                    Kind::List(items) | Kind::Vector(items) | Kind::Set(items) => {
                        metadata_pending.extend(items.iter().rev())
                    }
                    Kind::Conditional(_)
                    | Kind::SplicingConditional(_)
                    | Kind::Discard(_)
                    | Kind::Prefix { .. } => {
                        return Err(SessionError::Host(wasmtime::Error::msg(
                            "Native macro data requires resolved reader prefixes",
                        )));
                    }
                    _ => {}
                }
            }
        }
        pending.extend(form.metadata.iter().rev());
    }
    Ok(())
}

fn read(source: &str) -> Form {
    suss_reader::forms::read_forms(source).unwrap().remove(0)
}
fn outcome(result: Result<()>) -> String {
    match result {
        Ok(()) => "ok".into(),
        Err(SessionError::Host(error)) => format!("host:{error}"),
        Err(SessionError::Compile(error)) => format!("compile:{error:?}"),
        Err(error) => panic!("unexpected semantic error: {error:?}"),
    }
}
fn compare(form: &Form) -> (u64, u64) {
    let original = form.clone();
    let (mut old_trace, mut new_trace) = (Vec::new(), Vec::new());
    let (mut old_units, mut new_units) = (0, 0);
    let old = owned_guard_oracle(form, &mut |work| { old_units += work; Ok(()) },
        &mut |node| old_trace.push(node.cloned()));
    let new = validate_form_data_visiting(form, &mut |work| { new_units += work; Ok(()) },
        &mut |node| new_trace.push(node.cloned()));
    assert_eq!(outcome(new), outcome(old));
    assert_eq!(new_trace, old_trace, "full Forms, spans, nested metadata and visit order");
    assert_eq!(form, &original);
    (old_units, new_units)
}

#[test]
fn borrowed_metadata_guard_matches_owned_visits_and_removes_payload_copy_work() {
    let deep = format!("^{{:payload [{}]}} [17]", (0..128)
        .map(|i| format!("[\"payload-{i}-😀\" {i}]"))
        .collect::<Vec<_>>().join(" "));
    let form = read(&deep);
    let (old, new) = compare(&form);
    assert!(new < old, "actual planned units: owned={old}, borrowed={new}");
    eprintln!("metadata guard planned units owned={old} borrowed={new}");
    for source in [
        "^:flag ^String ^{:tag Other :x [1 2]} [17]",
        "^{[1 2] [\"winner\"] :other 9} ^{(1 2) [\"loser\"]} [17]",
        "^{^{:key-meta [1 2]} k ^{:value-meta [3 4]} v} [17]",
    ] { compare(&read(source)); }
    let mut deep_key = read(":deep");
    for _ in 0..128 {
        deep_key = Form { span: 5..9, metadata: vec![], kind: Kind::Vector(vec![deep_key]) };
    }
    let mut deep_collision = read("[17]");
    deep_collision.metadata = vec![
        Form { span: 1..4, metadata: vec![], kind: Kind::Map(vec![deep_key.clone(), read("[1 2]")]) },
        Form { span: 10..14, metadata: vec![], kind: Kind::Map(vec![deep_key, read("[3 4]")]) },
    ];
    compare(&deep_collision);
    // Raw UTF16 units, including lone surrogates, are validated without decoding.
    let mut utf16 = read("^{:x [\"x\"]} [17]");
    if let Kind::Map(pairs) = &mut utf16.metadata[0].kind {
        pairs[1] = Form { span: 7..11, metadata: vec![],
            kind: Kind::String(vec![0, 0xd800, 0xdc00, 0xffff]) };
    } else { panic!("metadata map"); }
    compare(&utf16);
}

#[test]
fn borrowed_metadata_guard_preserves_late_prefix_and_inner_metadata_error_order() {
    // Normalization must reject a losing malformed prefix before visiting winners.
    let mut late = read("^{:x 1} [17]");
    late.metadata.push(read("false"));
    compare(&late); // invalid prefix is first in the reverse normalizer
    let mut invalid_last = read("^{:x [1 2 3]} [17]");
    invalid_last.metadata.insert(0, read("false"));
    compare(&invalid_last); // valid inner map is normalized BEFORE invalid outer prefix
    let mut first_work = 0;
    let mut last_work = 0;
    let first_error = validate_form_data(&late, &mut |work| { first_work += work; Ok(()) });
    let last_error = validate_form_data(&invalid_last, &mut |work| { last_work += work; Ok(()) });
    assert!(matches!(first_error, Err(SessionError::Compile(_))));
    assert!(matches!(last_error, Err(SessionError::Compile(_))));
    assert!(last_work > first_work, "valid normalization work precedes the late invalid prefix");
    let mut odd = read("^{:x 1} [17]");
    if let Kind::Map(pairs) = &mut odd.metadata[0].kind { pairs.pop(); }
    compare(&odd);
    // The inner normalized walk deliberately ignores a winner's metadata.
    // Outer original-prefix traversal later performs the full metadata guards.
    let mut nested = read("^{:x [1]} [17]");
    if let Kind::Map(pairs) = &mut nested.metadata[0].kind {
        pairs[1].metadata.push(read("false"));
    }
    compare(&nested);
    let mut bad_target = read("17");
    bad_target.metadata.push(read("{:x 1}"));
    compare(&bad_target);
    // Losing values still receive the original-prefix validation, in old order.
    let mut losing = read("^{:x 1} ^{:x [2]} [17]");
    if let Kind::Map(pairs) = &mut losing.metadata[1].kind {
        pairs[1] = Form { span: 31..32, metadata: vec![], kind: Kind::Map(vec![read(":odd")]) };
    }
    compare(&losing);
    // All unresolved reader kinds must fail both as normalized winners and as
    // losing original-prefix values (the latter are visited only by the outer loop).
    for kind in [
        Kind::Conditional(vec![read(":cljs"), read("1")]),
        Kind::SplicingConditional(vec![read(":cljs"), read("[1]")]),
        Kind::Discard(Box::new(read("1"))),
        Kind::Prefix { operator: Box::new(read("quote")), target: Box::new(read("x")) },
    ] {
        let raw = Form { span: 41..49, metadata: vec![], kind };
        for winner in [false, true] {
            let mut form = read("^{:x 1} ^{:x 2} [17]");
            let prefix = if winner { 0 } else { 1 };
            if let Kind::Map(pairs) = &mut form.metadata[prefix].kind { pairs[1] = raw.clone(); }
            compare(&form);
            match validate_form_data(&form, &mut |_| Ok(())) {
                Err(SessionError::Host(error)) => assert_eq!(error.to_string(),
                    "Native macro data requires resolved reader prefixes"),
                other => panic!("raw winner={winner}: {other:?}"),
            }
        }
    }
}

#[test]
fn borrowed_metadata_guard_every_charge_preserves_typed_error_and_retry() {
    let form = read("^:flag ^String ^{:x [[1 2 3] [4 5 6]] :y \"😀\"} [17]");
    let original = form.clone();
    let mut charges = Vec::new();
    validate_form_data(&form, &mut |work| { charges.push(work); Ok(()) }).unwrap();
    for foreign in [false, true] {
        for fail_at in 0..charges.len() {
            let mut calls = 0;
            let result = validate_form_data(&form, &mut |_| {
                let current = calls;
                calls += 1;
                if current == fail_at {
                    Err(if foreign { SessionError::ForeignValue } else {
                        SessionError::Trap(wasmtime::Error::new(wasmtime::Trap::OutOfFuel))
                    })
                } else { Ok(()) }
            });
            assert_eq!(calls, fail_at + 1, "no work after failure at {fail_at}");
            match result {
                Err(SessionError::ForeignValue) if foreign => {},
                Err(SessionError::Trap(error)) if !foreign =>
                    assert_eq!(error.downcast_ref::<wasmtime::Trap>(), Some(&wasmtime::Trap::OutOfFuel)),
                other => panic!("typed failure {foreign}/{fail_at}: {other:?}"),
            }
            assert_eq!(form, original);
            let mut retry = Vec::new();
            validate_form_data(&form, &mut |work| { retry.push(work); Ok(()) }).unwrap();
            assert_eq!(retry, charges, "full identical retry after {foreign}/{fail_at}");
        }
    }
}
