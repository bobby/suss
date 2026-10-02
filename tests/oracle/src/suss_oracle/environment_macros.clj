(ns suss-oracle.environment-macros)
;; Development-only inspection of the pinned analyzer's actual macro environment.
(defmacro position []
  [(:line &env) (:column &env)])
