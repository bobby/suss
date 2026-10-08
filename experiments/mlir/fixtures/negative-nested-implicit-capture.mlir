// expected: external value
// Original Suss spike fixture, MIT/Apache-2.0.
module { %f = "suss.closure"() ({ ^entry(%x: f64): %g = "suss.closure"() ({ "suss.return"(%x) : (f64) -> () }) : () -> !suss.closure<() -> f64, []> "suss.return"(%g) : (!suss.closure<() -> f64, []>) -> () }) : () -> !suss.closure<(f64) -> !suss.closure<() -> f64, []>, []> }
