(core/defmacro array [& rest]
  (core/let [xs-str (core/->> (repeat "~{}")
                      (take (count rest))
                      (interpose ",")
                      (apply core/str))]
    (vary-meta
      (list* 'js* (core/str "[" xs-str "]") rest)
      assoc :tag 'array)))
