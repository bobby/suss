(ns suss-oracle.macro-metadata-defs)
;; Original development-only macro oracle. Same bodies execute compiled in Suss.
(defmacro form-tag [] (get (meta &form) :probe))
(defmacro form-line [] (get (meta &form) :line))
