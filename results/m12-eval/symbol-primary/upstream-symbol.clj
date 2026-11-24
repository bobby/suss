(defn symbol
  "Returns a Symbol with the given namespace and name. Arity-1 works
  on strings, keywords, and vars."
  {:tag clojure.lang.Symbol
   :added "1.0"
   :static true}
  ([name]
     (cond
      (symbol? name) name
      (instance? String name) (clojure.lang.Symbol/intern name)
      (instance? clojure.lang.Var name) (.toSymbol ^clojure.lang.Var name)
      (instance? clojure.lang.Keyword name) (.sym ^clojure.lang.Keyword name)
      :else (throw (IllegalArgumentException. "no conversion to symbol"))))
  ([ns name] (clojure.lang.Symbol/intern ns name)))