// expected: callee must have a suss closure type
module {
 %x = "suss.const"() {value = 1.0 : f64} : () -> f64
 %r = "suss.call"(%x) : (f64) -> f64 }