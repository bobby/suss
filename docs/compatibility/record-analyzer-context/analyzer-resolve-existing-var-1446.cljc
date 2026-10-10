(defn resolve-existing-var
  "Given env, an analysis environment, and sym, a symbol, resolve an existing var.
   Emits a warning if no such var exists."
  [env sym]
  (if-not (-> sym meta ::no-resolve)
    (resolve-var env sym confirm-var-exists)
    (resolve-var env sym)))
