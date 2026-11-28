//! Execute nested associative core helpers against pinned JVM observations.
use suss_cli::portable_session::Session;
fn printed(session: &mut Session, source: &str) -> String {
    let value = session.eval(source).unwrap();
    session.collect().unwrap();
    session
        .inspect(&value, |mut store, value| {
            let string = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            let units = string
                .elems(&mut store)?
                .map(|v| v.unwrap_i32() as u16)
                .collect::<Vec<_>>();
            Ok(String::from_utf16(&units).unwrap())
        })
        .unwrap()
}
#[test]
fn get_in_and_assoc_in_match_primary_nested_paths_in_both_compiled_phases() {
    let cases: Vec<(String, String)> = serde_json::from_str(include_str!(
        "../../../results/m5-nested-associative/primary/cases.json"
    ))
    .unwrap();
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        for (source, expected) in &cases {
            assert_eq!(
                printed(&mut session, &format!("(pr-str {source})")),
                *expected,
                "{source}"
            );
        }
    }
}
