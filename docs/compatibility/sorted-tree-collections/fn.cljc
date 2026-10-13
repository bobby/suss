(core/defmacro fn
     "params => positional-params* , or positional-params* & rest-param
     positional-param => binding-form
     rest-param => binding-form
     binding-form => name, or destructuring-form

     Defines a function

     See https://clojure.org/reference/special_forms#fn for more information"
     {:forms '[(fn name? [params*] exprs*) (fn name? ([params*] exprs*) +)]}
     [& sigs]
     (core/let [name (if (core/symbol? (first sigs)) (first sigs) nil)
                sigs (if name (next sigs) sigs)
                sigs (if (vector? (first sigs))
                       (core/list sigs)
                       (if (seq? (first sigs))
                         sigs
                         ;; Assume single arity syntax
                         (throw (js/Error.
                                  (if (seq sigs)
                                    (core/str "Parameter declaration "
                                      (core/first sigs)
                                      " should be a vector")
                                    (core/str "Parameter declaration missing"))))))
                psig (fn* [sig]
                       ;; Ensure correct type before destructuring sig
                       (core/when (not (seq? sig))
                         (throw (js/Error.
                                  (core/str "Invalid signature " sig
                                    " should be a list"))))
                       (core/let [[params & body] sig
                                  _ (core/when (not (vector? params))
                                      (throw (js/Error.
                                               (if (seq? (first sigs))
                                                 (core/str "Parameter declaration " params
                                                   " should be a vector")
                                                 (core/str "Invalid signature " sig
                                                   " should be a list")))))
                                  conds (core/when (core/and (next body) (map? (first body)))
                                          (first body))
                                  body (if conds (next body) body)
                                  conds (core/or conds (meta params))
                                  pre (:pre conds)
                                  post (:post conds)
                                  body (if post
                                         `((let [~'% ~(if (core/< 1 (count body))
                                                        `(do ~@body)
                                                        (first body))]
                                             ~@(map (fn* [c] `(assert ~c)) post)
                                             ~'%))
                                         body)
                                  body (if pre
                                         (concat (map (fn* [c] `(assert ~c)) pre)
                                           body)
                                         body)]
                         (maybe-destructured params body)))
                new-sigs (map psig sigs)]
       (with-meta
         (if name
           (list* 'fn* name new-sigs)
           (cons 'fn* new-sigs))
         (meta &form))))
