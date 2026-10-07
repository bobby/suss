(ns suss.harness.portability-macros
  "Suss counterpart of the suite's portability macros (clojure.core-test.portability).")

;; Upstream skips a test when its var does not resolve. Suss cannot yet query
;; var existence from macro code, so the body always expands: a var Suss lacks
;; fails its namespace with an exact diagnostic, and the oracle's skip is then a
;; recorded mismatch. Suss never reports a skip it did not prove.
(defmacro when-var-exists [_var-sym & body]
  `(do ~@body))
