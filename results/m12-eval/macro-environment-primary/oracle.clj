(println (pr-str (pr-str (do (eval '(defmacro env-top [] (list 'quote [(nil? &env) (first &form)]))) (eval '(env-top))))))
(println (pr-str (pr-str (do (eval '(defmacro env-local [] (list 'quote [(nil? &env) (contains? &env 'x) (count &env)]))) (eval '(let [x 7] (env-local)))))))
(println (pr-str (pr-str (eval '(let [x 1] (let [x 2 y 3] (env-local)))))))
