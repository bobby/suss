// expected: closure signature requires exactly one result
// Original Suss spike fixture, MIT/Apache-2.0.
module { %f = "suss.closure"() ({ ^entry(%arg: !suss.closure<() -> (f64, f64), []>): "suss.return"(%arg) : (!suss.closure<() -> (f64, f64), []>) -> () }) : () -> !suss.closure<(!suss.closure<() -> (f64, f64), []>) -> !suss.closure<() -> (f64, f64), []>, []> }
