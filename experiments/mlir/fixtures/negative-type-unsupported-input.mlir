// expected: closure elements must be f64 or a suss closure type
// Original Suss spike fixture, MIT/Apache-2.0.
module attributes {suss.type_probe = !suss.closure<(i32) -> f64, []>} {}
