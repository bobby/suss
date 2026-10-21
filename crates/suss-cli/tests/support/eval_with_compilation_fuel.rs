//! Compile semantic test inputs with a separate, cumulative native allowance.
//! Runtime evaluation still uses the caller Session's configured budget.
#[path = "compilation_fuel.rs"]
mod compilation_fuel;
use suss_cli::portable_session::Session;

pub fn eval(
    session: &mut Session,
    source: &str,
) -> Result<suss_cli::portable_session::SessionValue, suss_cli::portable_session::SessionError> {
    struct Compilation(u64);
    impl suss_compile::portable::ExpansionHost for Compilation {
        fn expansion_work_fuel(&self) -> u64 {
            self.0
        }
        fn artifact_dependencies(&self) -> Option<Vec<(String, String)>> {
            Some(vec![])
        }
        fn charge_expansion_work(
            &mut self,
            work: u64,
            span: std::ops::Range<usize>,
        ) -> Result<(), suss_compile::portable::Diagnostic> {
            self.0 =
                self.0
                    .checked_sub(work)
                    .ok_or_else(|| suss_compile::portable::Diagnostic {
                        span,
                        message: "Semantic corpus compilation fuel exhausted".into(),
                    })?;
            Ok(())
        }
        fn expand(
            &mut self,
            _: &suss_reader::forms::Form,
            _: suss_compile::portable::ExpansionContext<'_>,
        ) -> Result<Option<suss_reader::forms::Form>, suss_compile::portable::Diagnostic> {
            Ok(None)
        }
    }
    let runtime_fuel = session.options().fuel_per_operation;
    let result = session.eval_with_macros(source, &mut Compilation(compilation_fuel::COMPILATION_FUEL));
    assert_eq!(session.options().fuel_per_operation, runtime_fuel);
    result
}
