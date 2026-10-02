(ns suss-oracle.environment-macros)
;; Development-only inspection of the pinned analyzer's actual macro environment.
(defmacro position []
  [(:line &env) (:column &env)])
(defmacro inspect-local [sym]
  (let [binding (get-in &env [:locals sym])]
    [(:line binding) (:column binding)]))
