// expected: capture count/type mismatch
// Original Suss spike fixture, MIT/Apache-2.0.
module {
  %x = "suss.const"() {value = 7.0 : f64} : () -> f64 loc("capture.sus":1:9)
  %f = "suss.closure"(%x) ({
  ^entry(%captured: f64, %arg: f64):
    %sum = "suss.add"(%captured, %arg) : (f64, f64) -> f64 loc("capture.sus":1:24)
    "suss.return"(%sum) : (f64) -> ()
  }) : (f64) -> !suss.closure<(f64) -> f64, [!suss.closure<() -> f64, []>]> loc("capture.sus":1:15)
  %n = "suss.const"() {value = 2.0 : f64} : () -> f64
  %result = "suss.call"(%f, %n) : (!suss.closure<(f64) -> f64, [!suss.closure<() -> f64, []>]>, f64) -> f64 loc("capture.sus":2:1)
}
