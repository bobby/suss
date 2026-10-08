// Original Suss spike fixture, MIT/Apache-2.0.
module {
 %f = "suss.closure"() ({ ^entry(%arg: f64):
  "suss.return"(%arg) : (f64) -> ()
 }) : () -> !suss.closure<(f64) -> f64, []>
 %x = "suss.const"() {value = 3.0 : f64} : () -> f64
 %r = "suss.call"(%f, %x) : (!suss.closure<(f64) -> f64, []>, f64) -> f64
}
