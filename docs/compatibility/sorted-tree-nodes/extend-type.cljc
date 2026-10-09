(core/defmacro extend-type
  "Extend a type to a series of protocols. Useful when you are
  supplying the definitions explicitly inline. Propagates the
  type as a type hint on the first argument of all fns.

  type-sym may be

   * default, meaning the definitions will apply for any value,
     unless an extend-type exists for one of the more specific
     cases below.
   * nil, meaning the definitions will apply for the nil value.
   * any of object, boolean, number, string, array, or function,
     indicating the definitions will apply for values of the
     associated base JavaScript types. Note that, for example,
     string should be used instead of js/String.
   * a JavaScript type not covered by the previous list, such
     as js/RegExp.
   * a type defined by deftype or defrecord.

  (extend-type MyType
    ICounted
    (-count [c] ...)
    Foo
    (bar [x y] ...)
    (baz ([x] ...) ([x y] ...) ...)"
  [type-sym & impls]
  (core/let [env &env
             _ (validate-impls env impls)
             resolve (partial resolve-var env)
             impl-map (->impl-map impls)
             impl-map (if ('#{boolean number} type-sym)
                        (type-hint-impl-map type-sym impl-map)
                        impl-map)
             [type assign-impls] (core/if-let [type (base-type type-sym)]
                                   [type base-assign-impls]
                                   [(resolve type-sym) proto-assign-impls])]
    (core/when (core/and (:extending-base-js-type cljs.analyzer/*cljs-warnings*)
            (js-base-type type-sym))
      (cljs.analyzer/warning :extending-base-js-type env
        {:current-symbol type-sym :suggested-symbol (js-base-type type-sym)}))
    `(do ~@(mapcat #(assign-impls env resolve type-sym type %) impl-map))))
