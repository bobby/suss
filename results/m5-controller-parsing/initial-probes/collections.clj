(assert (= "1.12.6" (clojure-version)))
(let [xs (java.util.ArrayList. [1 2])]
  (prn {:clojure-version (clojure-version)
        :count (count xs) :seq (vec (seq xs))
        :conj (try [:ok (conj xs 3)] (catch Exception e [:error (.getName (class e))]))
        :into-vector (conj (into [] xs) 3)}))
