(require '[clojure.walk :as walk])
(let [source "(let [old nth effects (atom 9)] (try (set! nth (fn replacement ([v i] (do (swap! effects (fn [n] (+ (* n 10) i))) (set! nth old) (try (old v i) (finally (set! nth replacement))))) ([v i missing] (do (swap! effects (fn [n] (+ (* n 10) i))) (set! nth old) (try (old v i missing) (finally (set! nth replacement))))))) [((fn [[a b]] [a b]) [17 23]) @effects] (finally (set! nth old))))"
      form (read-string source)
      adapted (walk/postwalk
                (fn [form]
                  (if (and (seq? form) (= 'set! (first form)) (= 'nth (second form)))
                    (list 'alter-var-root (list 'var 'nth) (list 'fn ['_] (nth form 2)))
                    form)) form)
      value (eval adapted)]
  (prn adapted)
  (prn value)
  (assert (= [[17 23] 9] value)))
