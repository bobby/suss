(assert (= "1.12.6" (clojure-version)))
(require '[clojure.test :as t]
         'clojure.core-test.denominator
         'clojure.core-test.numerator
         'clojure.core-test.rationalize)
(let [result (t/run-tests 'clojure.core-test.denominator
                          'clojure.core-test.numerator
                          'clojure.core-test.rationalize)]
  (assert (= {:test 3 :pass 36 :fail 0 :error 0} (select-keys result [:test :pass :fail :error])))
  (println (pr-str result)))
