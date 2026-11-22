(assert (= "1.12.6" (clojure-version)))
(doseq [v [#'clojure.core/+' #'clojure.core/-' #'clojure.core/*' #'clojure.core/inc' #'clojure.core/dec']]
 (prn [(:name (meta v)) (boolean (:inline (meta v)))
       (mapv (fn [n] (if-let [a (:inline-arities (meta v))] (boolean (a n)) :all)) (range 5))]))
(prn [(with-redefs [+' (fn [& xs] :rebound)] (+' 1 2))
      (with-redefs [+' (fn [& xs] :rebound)] (+' 1 2 3))
      (with-redefs [inc' (fn [x] :rebound)] (inc' 1))])
