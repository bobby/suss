(ns clojure.core-test.num (:require [clojure.test :as t :refer [are deftest is testing]] [clojure.core-test.portability :refer [when-var-exists] :as p]))
(defn f [])
(when-var-exists num (deftest test-num (testing "positive cases" (is (NaN? (num ##NaN))) (are [n] (and (= n (num n)) (= (type n) (type (num n)))) 0 0.1 1/2 1N 1.0M (short 1) (byte 1) (int 1) (long 1) (float 1.0) (double 1.0) nil ##Inf) (testing "exceptions thrown" (are [x] (p/thrown? (num x)) (fn []) f {} #{} [] (quote ()) \1 \a "" "1" (quote a) #"")))))
