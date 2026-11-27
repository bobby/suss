(assert (= "1.12.6" (clojure-version)))
(prn (do (def ^:dynamic *snapshot* 1) (binding [*snapshot* (do (set! *snapshot* 2) 7)] *snapshot*) *snapshot*))
