(def ^:const frozen-long 9223372036854775807) (def read-frozen-long (fn [] frozen-long)) (def frozen-long 7) [(read-frozen-long) frozen-long]
(def ^:const frozen-double -0.0) (def read-frozen-double (fn [] frozen-double)) (def frozen-double 1.5) [(read-frozen-double) frozen-double]
(def ^:const frozen-bigint 9223372036854775808123N) (def read-frozen-bigint (fn [] frozen-bigint)) (def frozen-bigint 0) [(read-frozen-bigint) frozen-bigint]
(def ^:const frozen-ratio 22/7) (def read-frozen-ratio (fn [] frozen-ratio)) (def frozen-ratio 0) [(read-frozen-ratio) frozen-ratio]
(def ^:const frozen-text "hello") (def read-frozen-text (fn [] frozen-text)) (def frozen-text "later") [(read-frozen-text) frozen-text]
(def ^:const frozen-nil nil) (def read-frozen-nil (fn [] frozen-nil)) (def frozen-nil true) [(read-frozen-nil) frozen-nil]
(def ^:const frozen-bool false) (def read-frozen-bool (fn [] frozen-bool)) (def frozen-bool true) [(read-frozen-bool) frozen-bool]
(def ^:const removed-const 1) (def old-reader (fn [] removed-const)) (def removed-const 2) (def live-reader (fn [] removed-const)) (def removed-const 3) [(old-reader) (live-reader) removed-const]
(def ^{:const false} live-false 1) (def read-live-false (fn [] live-false)) (def live-false 2) [(read-live-false) live-false]
(def ^{:const nil} live-nil 1) (def read-live-nil (fn [] live-nil)) (def live-nil 2) [(read-live-nil) live-nil]
(def ^:const local-shadow 1) (let [local-shadow 42] [local-shadow ((fn [] local-shadow))])
(def ^:const boundary-long -9223372036854775808) (def ^:const boundary-max 1.7976931348623157e308) (def ^:const boundary-min 4.9e-324) [boundary-long boundary-max boundary-min]
(def ^:const identity-big 999999999999999999999N) (def ^:const identity-ratio 1/3) (def ^:const identity-text "long text") [(identical? identity-big identity-big) (identical? identity-ratio identity-ratio) (identical? identity-text identity-text)]
(def ^:const original-constant 999999999999999999N) (def ^:const aliased-constant original-constant) (def original-constant 0) [aliased-constant original-constant (identical? aliased-constant aliased-constant)]
(def ^:const different-constant-a 1/3) (def ^:const different-constant-b 1/3) [(identical? different-constant-a different-constant-b) (identical? 1/3 1/3)]
(def ^:const object-constant (quote 1/3)) (def object-constant-reader (fn [] object-constant)) (def object-constant 0) [(object-constant-reader) object-constant]
(def ^:const conversion-constant 5.5) (def conversion-reader (fn [] [(int conversion-constant) (long conversion-constant) conversion-constant])) (def conversion-constant 7) (conversion-reader)
(def ^:const wide-conversion-constant 1.0E30) (def wide-conversion-reader (fn [] [(try (int wide-conversion-constant) (catch Throwable e :range)) (try (long wide-conversion-constant) (catch Throwable e :range))])) (def wide-conversion-constant 1) (wide-conversion-reader)
(def ^:const primitive-return-long 7) (def primitive-return-ok (fn ^long [] primitive-return-long)) (primitive-return-ok)
