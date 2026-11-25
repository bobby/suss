(assert (= "1.12.6" (clojure-version)))
(let [value (do (def ^:dynamic *snapshot* 1) (binding [*snapshot* 1] (binding [*snapshot* (do (set! *snapshot* 2) 7)] *snapshot*) *snapshot*))] (assert (instance? Long value)) (println (str "{\"tag\":\"long\",\"value\":\"" value "\"}")))
(let [value (do (def ^:dynamic *defined* 1) (binding [*defined* 7] (def *defined* 9)) *defined*)] (assert (instance? Long value)) (println (str "{\"tag\":\"long\",\"value\":\"" value "\"}")))
