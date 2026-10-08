// expected: requires a suss closure result type
// Original Suss spike fixture, MIT/Apache-2.0.
module { %f = "suss.closure"() ({ %x = "suss.const"() {value = 1.0 : f64} : () -> f64 "suss.return"(%x) : (f64) -> () }) : () -> f64 }
