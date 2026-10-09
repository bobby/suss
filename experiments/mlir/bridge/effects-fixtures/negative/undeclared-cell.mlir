module attributes {suss.cells = [{namespace="effects", name="journal", initial_bits="0000000000000000"}, {namespace="effects", name="count", initial_bits="0000000000000000"}]} {
%entry = "suss.effect_closure"() ({
%body = "suss.effect_closure"() ({
%condition = "suss.effect_closure"() ({
%r1 = "suss.read"() {namespace_name="effects", name="missing"} : () -> !suss.value
%ten1 = "suss.const"() {value = 10.0 : f64} : () -> f64
%m1 = "suss.dynamic_arithmetic"(%r1, %ten1) {kind="multiply"} : (!suss.value, f64) -> !suss.value
%d1 = "suss.const"() {value = 1.0 : f64} : () -> f64
%a1 = "suss.dynamic_arithmetic"(%m1, %d1) {kind="add"} : (!suss.value, f64) -> !suss.value
%w1 = "suss.write"(%a1) {namespace_name="effects", name="journal"} : (!suss.value) -> !suss.value
%count = "suss.read"() {namespace_name="effects", name="count"} : () -> !suss.value
%one = "suss.const"() {value = 1.0 : f64} : () -> f64
%inc = "suss.dynamic_arithmetic"(%count, %one) {kind="add"} : (!suss.value, f64) -> !suss.value
%publish = "suss.write"(%inc) {namespace_name="effects", name="count"} : (!suss.value) -> !suss.value
%flag = "suss.bool"() {value=true} : () -> i1
"suss.effect_return"(%flag) : (i1) -> ()
}) : () -> !suss.effect_closure<() -> !suss.value, []>
%test = "suss.effect_call"(%condition) : (!suss.effect_closure<() -> !suss.value, []>) -> !suss.value
%true = "suss.bool"() {value=true} : () -> i1
%eq = "suss.equal"(%test, %true) : (!suss.value, i1) -> i1
%choice = "suss.if"(%eq) ({
%r2 = "suss.read"() {namespace_name="effects", name="journal"} : () -> !suss.value
%ten2 = "suss.const"() {value = 10.0 : f64} : () -> f64
%m2 = "suss.dynamic_arithmetic"(%r2, %ten2) {kind="multiply"} : (!suss.value, f64) -> !suss.value
%d2 = "suss.const"() {value = 2.0 : f64} : () -> f64
%a2 = "suss.dynamic_arithmetic"(%m2, %d2) {kind="add"} : (!suss.value, f64) -> !suss.value
%w2 = "suss.write"(%a2) {namespace_name="effects", name="journal"} : (!suss.value) -> !suss.value
%yes = "suss.const"() {value = 7.0 : f64} : () -> f64
"suss.yield"(%yes) : (f64) -> ()
}, {
%r7 = "suss.read"() {namespace_name="effects", name="journal"} : () -> !suss.value
%ten7 = "suss.const"() {value = 10.0 : f64} : () -> f64
%m7 = "suss.dynamic_arithmetic"(%r7, %ten7) {kind="multiply"} : (!suss.value, f64) -> !suss.value
%d7 = "suss.const"() {value = 7.0 : f64} : () -> f64
%a7 = "suss.dynamic_arithmetic"(%m7, %d7) {kind="add"} : (!suss.value, f64) -> !suss.value
%w7 = "suss.write"(%a7) {namespace_name="effects", name="journal"} : (!suss.value) -> !suss.value
%no = "suss.const"() {value = 8.0 : f64} : () -> f64
"suss.yield"(%no) : (f64) -> ()
}) : (i1) -> !suss.value
"suss.effect_return"(%choice) : (!suss.value) -> ()
}) : () -> !suss.effect_closure<() -> !suss.value, []>
%handler = "suss.effect_closure"() ({
^bb0(%payload: !suss.value):
%r3 = "suss.read"() {namespace_name="effects", name="journal"} : () -> !suss.value
%ten3 = "suss.const"() {value = 10.0 : f64} : () -> f64
%m3 = "suss.dynamic_arithmetic"(%r3, %ten3) {kind="multiply"} : (!suss.value, f64) -> !suss.value
%d3 = "suss.const"() {value = 3.0 : f64} : () -> f64
%a3 = "suss.dynamic_arithmetic"(%m3, %d3) {kind="add"} : (!suss.value, f64) -> !suss.value
%w3 = "suss.write"(%a3) {namespace_name="effects", name="journal"} : (!suss.value) -> !suss.value
"suss.effect_return"(%payload) : (!suss.value) -> ()
}) : () -> !suss.effect_closure<(!suss.value) -> !suss.value, []>
%cleanup = "suss.effect_closure"() ({
%r4 = "suss.read"() {namespace_name="effects", name="journal"} : () -> !suss.value
%ten4 = "suss.const"() {value = 10.0 : f64} : () -> f64
%m4 = "suss.dynamic_arithmetic"(%r4, %ten4) {kind="multiply"} : (!suss.value, f64) -> !suss.value
%d4 = "suss.const"() {value = 4.0 : f64} : () -> f64
%a4 = "suss.dynamic_arithmetic"(%m4, %d4) {kind="add"} : (!suss.value, f64) -> !suss.value
%w4 = "suss.write"(%a4) {namespace_name="effects", name="journal"} : (!suss.value) -> !suss.value
%clean = "suss.const"() {value = 0.0 : f64} : () -> f64
"suss.effect_return"(%clean) : (f64) -> ()
}) : () -> !suss.effect_closure<() -> !suss.value, []>
%result = "suss.try"(%body,%handler,%cleanup) : (!suss.effect_closure<() -> !suss.value, []>, !suss.effect_closure<(!suss.value) -> !suss.value, []>, !suss.effect_closure<() -> !suss.value, []>) -> !suss.value
"suss.effect_return"(%result) : (!suss.value) -> ()
}) : () -> !suss.effect_closure<() -> !suss.value, []>
}
