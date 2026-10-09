module {
 %x = "suss.const"() {value = 7.0 : f64} : () -> f64
 %f = "suss.closure"() ({
  "probe.container"() ({
   %bad = "suss.add"(%x, %x) : (f64, f64) -> f64
   "probe.end"() : () -> ()
  }) : () -> ()
  %v = "suss.const"() {value = 1.0 : f64} : () -> f64
  "suss.return"(%v) : (f64) -> ()
 }) : () -> !suss.closure<() -> f64, []>
}
