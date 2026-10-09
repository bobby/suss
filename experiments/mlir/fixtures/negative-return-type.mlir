// expected: return type mismatch
module { %f = "suss.closure"() ({ ^entry(%arg: !suss.closure<() -> f64, []>):
 %x = "suss.const"() {value = 1.0 : f64} : () -> f64
 "suss.return"(%x) : (f64) -> ()
 }) : () -> !suss.closure<(!suss.closure<() -> f64, []>) -> !suss.closure<() -> f64, []>, []> }