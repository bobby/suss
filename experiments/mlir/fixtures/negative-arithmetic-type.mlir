// expected: operand #1 must be 64-bit float
module { %f = "suss.closure"() ({ ^entry(%arg: !suss.closure<() -> f64, []>):
 %x = "suss.const"() {value = 1.0 : f64} : () -> f64
 %r = "suss.add"(%x, %arg) : (f64, !suss.closure<() -> f64, []>) -> f64
 "suss.return"(%arg) : (!suss.closure<() -> f64, []>) -> ()
 }) : () -> !suss.closure<(!suss.closure<() -> f64, []>) -> !suss.closure<() -> f64, []>, []> }