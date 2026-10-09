(core/defmacro str_
  ([] "")
  ([x]
   (if (typed-expr? &env x '#{string})
     x
     (string-expr (core/list 'js* "cljs.core.str_(~{})" x))))
  ([x & ys]
   (core/let [interpolate (core/fn [x]
                            (if (typed-expr? &env x '#{string clj-nil})
                              "~{}"
                              "cljs.core.str_(~{})"))
              strs        (core/->> (core/list* x ys)
                            (map interpolate)
                            (interpose ",")
                            (apply core/str))]
     (string-expr (list* 'js* (core/str "[" strs "].join('')") x ys)))))
