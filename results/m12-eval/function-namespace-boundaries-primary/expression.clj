(defmacro forwarded-ns-ignore [] 42)
(defmacro forwarded-ns-value [] (list 'quote *ns*))

(prn [(binding [*ns* 42] ((deref #'user/forwarded-ns-ignore) nil nil))
 (binding [*ns* 42] (second ((deref #'user/forwarded-ns-value) nil nil)))
 (binding [*ns* (atom 42)] ((deref #'user/forwarded-ns-ignore) nil nil))]
)
