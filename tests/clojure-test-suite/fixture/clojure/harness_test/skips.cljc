(ns clojure.harness-test.skips
  "Harness self-test for load-time skip attribution."
  #?(:suss (:require [suss.harness.test])
     :cljs (:require [cljs.test]))
  #?(:suss (:require-macros [suss.harness.test-macros :refer [deftest is]])
     :cljs (:require-macros [cljs.test :refer [deftest is]])))

#?(:suss (suss.harness.test/skip! 'host-only-fn)
   :cljs (println "SKIP -" 'host-only-fn))

(deftest after-skip
  (is (= 1 1)))
