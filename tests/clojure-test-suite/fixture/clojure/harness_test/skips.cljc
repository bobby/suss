(ns clojure.harness-test.skips
  "Harness self-test: the oracle skips a test whose var it lacks. Suss never
  skips; the missing var fails this namespace and the skip is a mismatch."
  (:require [clojure.test :refer [deftest is]]
            [clojure.core-test.portability #?(:cljs :refer-macros :default :refer) [when-var-exists]]))

(when-var-exists host-only-fn
  (deftest uses-host-only-fn
    (is (host-only-fn))))

(deftest after-skip
  (is (= 1 1)))
