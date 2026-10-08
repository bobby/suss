// expected: closure signature requires exactly one result
// Original Suss spike fixture, MIT/Apache-2.0.
module attributes {suss.type_probe = !suss.closure<() -> !suss.closure<() -> (), []>, []>} {}
