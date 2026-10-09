(core/defn- add-ifn-methods [type type-sym [f & meths :as form]]
  (core/let [meths    (map #(adapt-ifn-params type %) meths)
             this-sym (with-meta 'self__ {:tag type})
             argsym   (gensym "args")
             max-ifn-arity 20]
    (concat
      [`(set! ~(extend-prefix type-sym 'call) ~(with-meta `(fn ~@meths) (meta form)))
       `(set! ~(extend-prefix type-sym 'apply)
          ~(with-meta
             `(fn ~[this-sym argsym]
                (this-as ~this-sym
                  (let [args# (cljs.core/aclone ~argsym)]
                    (.apply (.-call ~this-sym) ~this-sym
                      (.concat (array ~this-sym)
                        (if (> (.-length args#) ~max-ifn-arity)
                          (doto (.slice args# 0 ~max-ifn-arity)
                            (.push (.slice args# ~max-ifn-arity)))
                          args#))))))
             (meta form)))]
      (ifn-invoke-methods type type-sym form))))
