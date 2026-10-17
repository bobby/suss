//! Development metered-compilation cost attribution, never default acceptance.
use suss_compile::portable::{self, Diagnostic, ExpansionContext, ExpansionHost};
use suss_reader::forms::{Form, read_forms_metered, resolve_conditionals_metered};
use std::{collections::BTreeSet, ops::Range, path::PathBuf};

struct CostHost { remaining: u64, spent: u64, emission: u64 }
struct EmitProxy<'a>(&'a mut CostHost);
impl ExpansionHost for EmitProxy<'_> {
    fn expansion_work_fuel(&self) -> u64 { self.0.remaining }
    fn charge_expansion_work(&mut self, units: u64, span: Range<usize>) -> Result<(), Diagnostic> {
        self.0.charge_expansion_work(units, span)
    }
    fn expand(&mut self, _: &Form, _: ExpansionContext<'_>) -> Result<Option<Form>, Diagnostic> { Ok(None) }
}
impl ExpansionHost for CostHost {
    fn expansion_work_fuel(&self) -> u64 { self.remaining }
    fn charge_expansion_work(&mut self, units: u64, span: Range<usize>) -> Result<(), Diagnostic> {
        self.remaining = self.remaining.checked_sub(units)
            .ok_or_else(|| Diagnostic { span, message: "Diagnostic compiler budget exhausted".into() })?;
        self.spent += units;
        Ok(())
    }
    fn artifact_dependencies(&self) -> Option<Vec<(String, String)>> { Some(vec![]) }
    fn expand(&mut self, _: &Form, _: ExpansionContext<'_>) -> Result<Option<Form>, Diagnostic> { Ok(None) }
    fn emit_fragment(&mut self, function: &portable::ir::Function, phase: portable::resolve::Phase,
        forms: &[Form], origin: Option<&portable::SourceOrigin>) -> Result<Vec<u8>, Diagnostic> {
        let before = self.spent;
        let result = ExpansionHost::emit_fragment(&mut EmitProxy(self), function, phase, forms, origin);
        self.emission += self.spent - before;
        result
    }
}
fn main() {
    let limit: u64 = std::env::args().nth(1).expect("explicit diagnostic limit required").parse().unwrap();
    let source = include_str!("../../../runtime/core-import/suss/core.sus");
    let mut host = CostHost { remaining: limit, spent: 0, emission: 0 };
    // Same reader/conditional work and source-copy charge as modules::prepare_input.
    let forms = {
        let mut meter = |units| host.charge_expansion_work(units, 0..0)
            .map_err(|error| suss_reader::ParseError { span: error.span, message: error.message, expected: vec![] });
        read_forms_metered(source, &mut meter)
            .and_then(|forms| resolve_conditionals_metered(forms, &mut meter)).unwrap()
    };
    host.charge_expansion_work(source.len() as u64, 0..source.len()).unwrap();
    let prefix = host.spent;
    let phase = portable::resolve::Phase::Runtime;
    // Session::new's compile environment and provided namespace, without Store execution.
    let environment = portable::resolve::Environment::default();
    let provided = BTreeSet::from([portable::modules::ModuleIdentity::new(phase, "suss.core").unwrap()]);
    let result = portable::modules::prepare_input_forms_with_origin(forms, 0..source.len(),
        &[PathBuf::from("src")], &environment, phase, &provided, &mut host,
        Some(&portable::SourceOrigin::new(source, None)));
    eprintln!("NATIVE_COMPILER_COST default={} diagnostic={} acceptance=false prefix={} prepare_excluding_verify={} verify={} total={}",
        portable::DEFAULT_OPERATION_FUEL, limit, prefix, host.spent - prefix - host.emission,
        host.emission, host.spent);
    match result {
        Ok(input) => eprintln!("NATIVE_COMPILER_COST result=Ok wasm_bytes={} modules={} executed=false", input.fragment.wasm.len(), input.modules.len()),
        Err(error) => { eprintln!("NATIVE_COMPILER_COST result=Err({error:?})"); std::process::exit(1); }
    }
}
