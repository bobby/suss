(ns clojure.harness-test.basics
  "Harness self-test in the suite's shape, limited to what Suss executes today."
  #?(:suss (:require [suss.harness.test])
     :cljs (:require [cljs.test] [clojure.core-test.portability :as p]))
  #?(:suss (:require-macros [suss.harness.test-macros :refer [deftest is are testing]])
     :cljs (:require-macros [cljs.test :refer [deftest is are testing]]
                            [clojure.core-test.portability])))

;; Each top-level form that expands harness macros currently costs one
;; complete &env materialization (#14), so cases share few deftests.

(deftest predicates
  (testing "equality of scalars and collections"
    (is (= 3 (+ 1 2)))
    (is (= [1 2] (conj [1] 2)))
    (is (= {:a 1} (assoc {} :a 1)))
    (is (= #{:x} (conj #{} :x)))
    (is (= "ab" "ab")))
  (is (not (nil? 0)))
  (is (= -0.0 (- 0.0)))
  (is (= 2 (count (list 1 2))))
  (testing "opaque operands are judged by verdict"
    (is (fn? inc))))

(deftest values-and-throws
  (is true)
  (is (let [x 1] (= x 1)))
  (is (and true 1))
  (is :kw)
  (is (p/thrown? (throw (ex-info "boom" {:k 1}))))
  (is (= {:k 1} (try (throw (ex-info "x" {:k 1})) (catch :default e (ex-data e))))))

(deftest templates-and-effects
  (are [x y] (= x (inc y))
    2 1
    3 2
    0 -1)
  (let [log (atom [])]
    (is (= [1 2] [(do (swap! log conj :a) 1) (do (swap! log conj :b) 2)]))
    (is (= [:a :b] (deref log)))))
