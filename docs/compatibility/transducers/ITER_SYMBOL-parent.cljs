(if (and (exists? js/Symbol)
         (identical? (goog/typeOf js/Symbol) "function"))
  (def ITER_SYMBOL (.-iterator js/Symbol))
  (def ITER_SYMBOL "@@iterator"))
