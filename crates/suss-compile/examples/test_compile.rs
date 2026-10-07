//! Prepare and execute complete expression bundles in the native reference host.
#[cfg(not(target_family = "wasm"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use suss_compile::{Compiler, portable_repl, portable_session::Session};

    let mut compiler = Compiler::new();
    for source in [
        "(+ 1 2)",
        "(defn foo [x] (+ x 1)) (foo 5)",
        "(pr-str (defn foo [x] (+ x 1)) (foo 5))",
    ] {
        let artifact = compiler.compile_expr_with_info(source)?;
        let mut session = Session::new()?;
        let value = artifact
            .execute(&mut session)?
            .ok_or("expected an expression result")?;
        session.collect()?;
        println!(
            "{source} => {}",
            portable_repl::display(&mut session, &value)?
        );
    }
    Ok(())
}

#[cfg(target_family = "wasm")]
fn main() {
    panic!("This example requires the native reference host");
}
