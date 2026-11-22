(require '[clojure.walk :as walk])
(prn :compiler-options *compiler-options*)
(doseq [sym '[first next nth]] (prn sym (select-keys (meta (resolve sym)) [:inline :static])))
(let [form (read-string "(let [f first n next t nth effects (atom 0)] (try (set! first (fn [v] (do (swap! effects (fn [n] (+ (* n 10) 1))) (f v)))) (set! next (fn [v] (do (swap! effects (fn [n] (+ (* n 10) 2))) (n v)))) (set! nth (fn replacement ([v i] (do (swap! effects (fn [n] (+ (* n 10) 3))) (set! nth t) (try (t v i) (finally (set! nth replacement))))) ([v i missing] (do (swap! effects (fn [n] (+ (* n 10) 3))) (set! nth t) (try (t v i missing) (finally (set! nth replacement))))))) [((fn [[[a] & [b]]] [a b]) [[17] 23]) @effects] (finally (set! first f) (set! next n) (set! nth t))))")
      adapted (walk/postwalk (fn [form]
                 (if (and (seq? form) (= 'set! (first form)) (#{'first 'next 'nth} (second form)))
                   (list 'alter-var-root (list 'var (second form)) (list 'fn ['_] (nth form 2))) form)) form)
      value (eval adapted)]
 (prn adapted)
 (prn value)
 (assert (= [[17 23] 12] value)))
