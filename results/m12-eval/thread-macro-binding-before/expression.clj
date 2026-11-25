(assert (= "1.12.6" (clojure-version)))
(prn (do (declare dynamic-bound-macro) (let [v (eval '(defmacro ^:dynamic dynamic-bound-macro [] 1))] [(binding [dynamic-bound-macro (fn [& xs] 42)] [(.isMacro v) (eval '(dynamic-bound-macro))]) (eval '(dynamic-bound-macro))])))
