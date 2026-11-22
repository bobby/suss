(require '[clojure.test :as t] 'clojure.core-test.every-qmark)
(assert (= "1.12.6" (clojure-version)))
(let [result (t/run-tests 'clojure.core-test.every-qmark)]
  (prn result)
  (assert (= {:test 1 :pass 31 :fail 0 :error 0} result)))
