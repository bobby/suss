// expected: body must contain exactly one block
module { %f = "suss.closure"() ({}) : () -> !suss.closure<() -> f64, []> }
