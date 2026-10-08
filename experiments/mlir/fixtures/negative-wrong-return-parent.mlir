// expected: must terminate a suss.closure body
// Original Suss spike fixture, MIT/Apache-2.0.
module { %x = "suss.const"() {value = 1.0 : f64} : () -> f64 "suss.return"(%x) : (f64) -> () }
