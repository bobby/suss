// expected: expects region #0 to have 0 or 1 blocks
// Original Suss spike fixture, MIT/Apache-2.0.
module { %f = "suss.closure"() ({ ^a(%x: f64): "suss.return"(%x) : (f64) -> () ^b(%y: f64): "suss.return"(%y) : (f64) -> () }) : () -> !suss.closure<(f64) -> f64, []> }
