//! Explicit bridge compilation allowance preserves capture and Runtime defaults.
use suss_compile::{portable::resolve::Phase, portable_macro_data::FormBridge,
    portable_session::{Session, SessionError, SessionOptions}};
use suss_reader::forms::Kind;

#[test]
fn bridge_compile_fuel_zero_preserves_runtime_and_complete_recovery() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut session = Session::with_options_in(SessionOptions::default(), phase).unwrap();
        let runtime_fuel = session.options().fuel_per_operation;
        let before = session.stats();
        let error = FormBridge::new_with_compile_fuel(&mut session, Some(0)).err()
            .expect("zero compilation allowance must reject initialization");
        assert!(matches!(&error, SessionError::Compile(error) if error.message.contains("fuel")), "{error}");
        assert_eq!(session.options().fuel_per_operation, runtime_fuel);
        assert_eq!(session.stats().resident_fragments, before.resident_fragments);
        assert_eq!(session.stats().binding_cells, before.binding_cells);
        let bridge = FormBridge::new_with_compile_fuel(&mut session, Some(100_000_000_000)).unwrap();
        let original = FormBridge::new(&mut session).unwrap();
        let value = session.eval("42").unwrap();
        session.collect().unwrap();
        for capture in [&bridge, &original] {
            assert_eq!(capture.read(&mut session, &value, 0..1).unwrap().kind, Kind::Number(42.0));
        }
        assert_eq!(session.options().fuel_per_operation, runtime_fuel);
    }
}
