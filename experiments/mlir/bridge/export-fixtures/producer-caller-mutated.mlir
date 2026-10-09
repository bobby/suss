// Original evaluation fixture, MIT/Apache-2.0. Wrappers are transport roots.
module {
  %producer = "suss.closure"() ({
    %x = "suss.const"() {value = 11.0 : f64} : () -> f64
    %f = "suss.closure"(%x) ({
    ^entry(%captured: f64, %arg: f64):
      %sum = "suss.add"(%captured, %arg) : (f64, f64) -> f64
      "suss.return"(%sum) : (f64) -> ()
    }) : (f64) -> !suss.closure<(f64) -> f64, [f64]>
    "suss.return"(%f) : (!suss.closure<(f64) -> f64, [f64]>) -> ()
  }) : () -> !suss.closure<() -> !suss.closure<(f64) -> f64, [f64]>, []>
  %caller = "suss.closure"() ({
  ^entry(%binding: !suss.closure<(f64) -> f64, [f64]>):
    %n = "suss.const"() {value = 2.0 : f64} : () -> f64
    %result = "suss.call"(%binding, %n) : (!suss.closure<(f64) -> f64, [f64]>, f64) -> f64
    "suss.return"(%result) : (f64) -> ()
  }) : () -> !suss.closure<(!suss.closure<(f64) -> f64, [f64]>) -> f64, []>
}
