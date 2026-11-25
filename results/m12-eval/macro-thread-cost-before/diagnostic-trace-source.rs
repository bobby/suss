
#[test]
fn root_thread_macro_aggregate_fuel_trace() {
    let mut session = Session::new_repl().unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    macros.install_observation_hook(|store, _| {
        eprintln!("thread-aggregate remaining={}", store.get_fuel()?);
        Ok(0)
    }).unwrap();
    let rows: serde_json::Value = serde_json::from_str(include_str!("../../../results/m12-eval/thread-macro-binding-primary/command.json")).unwrap();
    let cases = rows["cases"].as_array().unwrap().iter().enumerate().map(|(index, row)| {
        let expression = row["expression"].as_str().unwrap().replace("(catch Throwable ", "(catch :default ");
        format!("(let [result__trace {expression}] (suss.harness.observation/capture {index}) result__trace)")
    }).collect::<Vec<_>>().join("\n");
    macros.define(&format!("(defmacro thread-trace-controls [] (suss.harness.observation/capture -1) (list 'quote [{cases}]))")).unwrap();
    eprintln!("thread-aggregate before_env={}", macros.environment_materializations());
    let result = session.eval_with_macros("(thread-trace-controls)", &mut macros);
    eprintln!("thread-aggregate after_env={}", macros.environment_materializations());
    match result {
        Ok(_) => eprintln!("thread-aggregate actual0"),
        Err(error) => panic!("thread-aggregate {}", suss_compile::portable_repl::error_display(&mut session, &error)),
    }
}
