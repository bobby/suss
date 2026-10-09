// Original Suss spike fixture, MIT/Apache-2.0.
module {
 %make = "suss.closure"() ({ ^entry(%x: f64):
  %inner = "suss.closure"(%x) ({ ^body(%capture: f64):
   "suss.return"(%capture) : (f64) -> ()
  }) : (f64) -> !suss.closure<() -> f64, [f64]>
  "suss.return"(%inner) : (!suss.closure<() -> f64, [f64]>) -> ()
 }) : () -> !suss.closure<(f64) -> !suss.closure<() -> f64, [f64]>, []>
 %apply = "suss.closure"() ({ ^entry(%fn: !suss.closure<() -> f64, [f64]>):
  %r = "suss.call"(%fn) : (!suss.closure<() -> f64, [f64]>) -> f64
  "suss.return"(%r) : (f64) -> ()
 }) : () -> !suss.closure<(!suss.closure<() -> f64, [f64]>) -> f64, []>
 %x = "suss.const"() {value = 7.0 : f64} : () -> f64
 %fn = "suss.call"(%make, %x) : (!suss.closure<(f64) -> !suss.closure<() -> f64, [f64]>, []>, f64) -> !suss.closure<() -> f64, [f64]>
 %holder = "suss.closure"(%fn) ({ ^entry(%capture: !suss.closure<() -> f64, [f64]>):
  "suss.return"(%capture) : (!suss.closure<() -> f64, [f64]>) -> ()
 }) : (!suss.closure<() -> f64, [f64]>) -> !suss.closure<() -> !suss.closure<() -> f64, [f64]>, [!suss.closure<() -> f64, [f64]>]>
 %saved = "suss.call"(%holder) : (!suss.closure<() -> !suss.closure<() -> f64, [f64]>, [!suss.closure<() -> f64, [f64]>]>) -> !suss.closure<() -> f64, [f64]>
 %result = "suss.call"(%apply, %saved) : (!suss.closure<(!suss.closure<() -> f64, [f64]>) -> f64, []>, !suss.closure<() -> f64, [f64]>) -> f64
}
