//! Ordinary method-root mutation; separate from reserved call/apply write probes.
use suss_compile::portable_session::Session;

#[test]
fn primitive_string_root_custom_field_mutation_during_index_preserves_selected_method() {
    let guest = include_str!("fixtures/primitive_string_root_success.cljs");
    // Pinned CLJS: two exit0 runs of guest SHA33fde62f...17e6f53 matched this row.
    let expected = "[66 17 true 66 [1 2 3 4]]";
    let mut failures = Vec::new();
    for (phase, mut session) in [
        ("runtime", Session::new_repl().unwrap()),
        ("macro", Session::new_macro().unwrap()),
    ] {
        match session.eval(guest) {
            Ok(value) => match suss_compile::portable_repl::display(&mut session, &value) {
                Ok(row) => {
                    eprintln!("root-success-probe {phase}: {row}");
                    if row != expected {
                        failures.push(format!("{phase}: {row} != {expected}"));
                    }
                }
                Err(error) => failures.push(format!("{phase}: decode failed {error:?}")),
            },
            Err(error) => failures.push(format!("{phase}: execution failed {error:?}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
