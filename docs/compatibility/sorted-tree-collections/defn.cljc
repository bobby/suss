(def
  ^{:doc "Same as (def name (core/fn [params* ] exprs*)) or (def
    name (core/fn ([params* ] exprs*)+)) with any doc-string or attrs added
    to the var metadata. prepost-map defines a map with optional keys
    :pre and :post that contain collections of pre or post conditions."
    :arglists '([name doc-string? attr-map? [params*] prepost-map? body]
                 [name doc-string? attr-map? ([params*] prepost-map? body)+ attr-map?])
    :macro true}
  defn (core/fn defn [&form &env name & fdecl]
         ;; Note: Cannot delegate this check to def because of the call to (with-meta name ..)
         (if (core/instance? #?(:clj clojure.lang.Symbol :cljs Symbol) name)
           nil
           (throw
             #?(:clj (IllegalArgumentException. "First argument to defn must be a symbol")
                :cljs (js/Error. "First argument to defn must be a symbol"))))
         (core/let [m (if (core/string? (first fdecl))
                        {:doc (first fdecl)}
                        {})
                    fdecl (if (core/string? (first fdecl))
                            (next fdecl)
                            fdecl)
                    m (if (map? (first fdecl))
                        (conj m (first fdecl))
                        m)
                    fdecl (if (map? (first fdecl))
                            (next fdecl)
                            fdecl)
                    fdecl (if (vector? (first fdecl))
                            (core/list fdecl)
                            fdecl)
                    m (if (map? (last fdecl))
                        (conj m (last fdecl))
                        m)
                    fdecl (if (map? (last fdecl))
                            (butlast fdecl)
                            fdecl)
                    m (conj {:arglists (core/list 'quote (sigs fdecl))} m)
                    ;; no support for :inline
                    ;m (core/let [inline (:inline m)
                    ;             ifn (first inline)
                    ;             iname (second inline)]
                    ;    ;; same as: (if (and (= 'fn ifn) (not (symbol? iname))) ...)
                    ;    (if (if #?(:clj (clojure.lang.Util/equiv 'fn ifn)
                    ;               :cljs (= 'fn ifn))
                    ;          (if #?(:clj (core/instance? clojure.lang.Symbol iname)
                    ;                 :cljs (core/instance? Symbol iname)) false true))
                    ;      ;; inserts the same fn name to the inline fn if it does not have one
                    ;      (assoc m
                    ;        :inline (cons ifn
                    ;                  (cons (clojure.lang.Symbol/intern
                    ;                          (.concat (.getName ^clojure.lang.Symbol name) "__inliner"))
                    ;                    (next inline))))
                    ;      m))
                    m (conj (if (meta name) (meta name) {}) m)]
           (core/cond
             (multi-arity-fn? fdecl)
             (multi-arity-fn name
               (if (comp/checking-types?)
                 (update-in m [:jsdoc] conj "@param {...*} var_args")
                 m) fdecl (:def-emits-var &env))

             (variadic-fn? fdecl)
             (variadic-fn name
               (if (comp/checking-types?)
                 (update-in m [:jsdoc] conj "@param {...*} var_args")
                 m) fdecl (:def-emits-var &env))

             :else
             (core/list 'def (with-meta name m)
               ;;todo - restore propagation of fn name
               ;;must figure out how to convey primitive hints to self calls first
               (cons `fn fdecl))))))
