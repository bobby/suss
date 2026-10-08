// expected: closure signature requires exactly one result
// Original Suss spike fixture, MIT/Apache-2.0.
module { %f = "suss.closure"() ({ ^entry(%arg: !suss.closure<() -> (), []>): "suss.return"(%arg) : (!suss.closure<() -> (), []>) -> () }) : () -> !suss.closure<(!suss.closure<() -> (), []>) -> !suss.closure<() -> (), []>, []> }
