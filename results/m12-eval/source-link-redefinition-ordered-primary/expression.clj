(def primary-first-result (load-string "(defmacro staged-reload \"first\" [] 17)\n(def held-staged-var #'staged-reload)\n[(identical? held-staged-var #'staged-reload) ((deref held-staged-var) nil nil) (:doc (meta held-staged-var))]\n"))
(alter-var-root held-staged-var (constantly (fn [& _] 42)))
(prn [primary-first-result (load-string "(defmacro staged-reload \"second\" [] 23)\n[(identical? held-staged-var #'staged-reload) ((deref held-staged-var) nil nil) (:doc (meta held-staged-var)) (eval '(staged-reload))]\n")])
