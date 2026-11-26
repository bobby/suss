(assert (= "1.12.6" (clojure-version)))
[(let [x 1 f (fn [n] (+ n x))] (try (eval (list 'identity f)) (catch Throwable e [(symbol (.getName (class e))) (:clojure.error/phase (ex-data e)) (some-> e .getCause .getMessage)])))
 (let [a (atom 0) f (fn [] (swap! a inc))] [(try (eval (list f)) (catch Throwable e [(symbol (.getName (class e))) (:clojure.error/phase (ex-data e)) (some-> e .getCause .getMessage)])) @a])]
