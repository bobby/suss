// Original evaluation regression, MIT/Apache-2.0. Valid dialect, unsupported harness.
module {
  %producer = "suss.closure"() ({
    %x = "suss.const"() {value = 7.0 : f64} : () -> f64
    "suss.return"(%x) : (f64) -> ()
  }) : () -> !suss.closure<() -> f64, []>
  %caller = "suss.closure"() ({
  ^entry(%binding: f64):
    "suss.return"(%binding) : (f64) -> ()
  }) : () -> !suss.closure<(f64) -> f64, []>
}
