(assert (= "1.12.6" (clojure-version)))
(prn (let [v (eval '(def ^{:macro false} metadata-false 42))] [(contains? (meta v) :macro) (:macro (meta v)) (.isMacro v)]))
