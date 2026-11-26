(assert (= "1.12.6" (clojure-version)))
(def unbound-replacement-target 42)
(def unbound-replacement-other)
(let [foreign @#'unbound-replacement-other]
  (alter-var-root #'unbound-replacement-target (fn [_] foreign))
  (prn [(identical? foreign @#'unbound-replacement-target)
        (.hasRoot #'unbound-replacement-target)]))
