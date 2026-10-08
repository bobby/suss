// expected: legacy dictionaries unsupported
module { %x = "suss.const"() {value = 1.0 : f64, suss.analysis = {binding = 0 : i64, children = [], metadata = []}} : () -> f64 }
