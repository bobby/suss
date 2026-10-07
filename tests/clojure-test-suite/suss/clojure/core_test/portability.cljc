(ns clojure.core-test.portability
  "Suss runtime counterpart of the suite's portability helpers. It replaces the
  upstream file in the harness source tree; the upstream file defines macros
  beside functions, which a Suss runtime namespace cannot yet do. Macros live
  in suss.harness.portability-macros and p/thrown? in suss.harness.test-macros/is.")

;; ClojureScript numbers are binary64; integer? is true of integral values.
(def big-int?
  (fn big-int? [n]
    (integer? n)))

;; The ClojureScript helper schedules a timer and returns immediately.
(def sleep
  (fn sleep [_ms]
    nil))

(def lazy-seq?
  (fn lazy-seq? [x]
    (instance? LazySeq x)))
