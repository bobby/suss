(ns clojure.harness-test.fixtures
  "Harness self-test: a :once fixture wraps every test of its namespace."
  (:require [clojure.test :refer [deftest is use-fixtures]]))

(def log (atom []))

(defn with-log [f]
  (swap! log conj :before)
  (f)
  (swap! log conj :after))

(use-fixtures :once with-log)

(deftest first-test
  (is (= [:before] (deref log))))

(deftest second-test
  (is (= [:before] (deref log))))
