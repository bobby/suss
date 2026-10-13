(core/defmacro case
  "Takes an expression, and a set of clauses.

  Each clause can take the form of either:

  test-constant result-expr

  (test-constant1 ... test-constantN)  result-expr

  The test-constants are not evaluated. They must be compile-time
  literals, and need not be quoted.  If the expression is equal to a
  test-constant, the corresponding result-expr is returned. A single
  default expression can follow the clauses, and its value will be
  returned if no clause matches. If no default expression is provided
  and no clause matches, an Error is thrown.

  Unlike cond and condp, case does a constant-time dispatch, the
  clauses are not considered sequentially.  All manner of constant
  expressions are acceptable in case, including numbers, strings,
  symbols, keywords, and (ClojureScript) composites thereof. Note that since
  lists are used to group multiple constants that map to the same
  expression, a vector can be used to match a list if needed. The
  test-constants need not be all of the same type."
  [e & clauses]
  (core/let [esym    (gensym)
             default (if (odd? (count clauses))
                       (last clauses)
                       `(throw
                          (js/Error.
                            (cljs.core/str "No matching clause: " ~esym))))
             env     &env
             pairs   (reduce
                       (core/fn [m [test expr]]
                         (core/cond
                           (seq? test)
                           (reduce
                             (core/fn [m test]
                               (core/let [test (if (core/symbol? test)
                                                 (core/list 'quote test)
                                                 test)]
                                 (assoc-test m test expr env)))
                             m test)
                           (core/symbol? test)
                           (assoc-test m (core/list 'quote test) expr env)
                           :else
                           (assoc-test m test expr env)))
                     {} (partition 2 clauses))
             tests   (keys pairs)]
    (core/cond
      (every? (some-fn core/number? core/string? #?(:clj core/char? :cljs (core/fnil core/char? :nonchar)) #(const? env %)) tests)
      (core/let [no-default (if (odd? (count clauses)) (butlast clauses) clauses)
                 tests      (mapv #(if (seq? %) (vec %) [%]) (take-nth 2 no-default))
                 thens      (vec (take-nth 2 (drop 1 no-default)))]
        `(let [~esym ~e] (case* ~esym ~tests ~thens ~default)))

      (every? core/keyword? tests)
      (core/let [no-default (if (odd? (count clauses)) (butlast clauses) clauses)
                 kw-str #(.substring (core/str %) 1)
                 tests (mapv #(if (seq? %) (mapv kw-str %) [(kw-str %)]) (take-nth 2 no-default))
                 thens (vec (take-nth 2 (drop 1 no-default)))]
        `(let [~esym ~e
               ~esym (if (keyword? ~esym) (.-fqn ~(vary-meta esym assoc :tag 'cljs.core/Keyword)) nil)]
           (case* ~esym ~tests ~thens ~default)))

      ;; equality
      :else
      `(let [~esym ~e]
         (cond
           ~@(mapcat (core/fn [[m c]] `((cljs.core/= ~m ~esym) ~c)) pairs)
           :else ~default)))))
