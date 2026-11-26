(defmacro nested-meta-remember [f] (eval (list 'def 'nested-meta-held f)) nil)
(defmacro nested-meta-check [f g]
  (let [prior (eval 'nested-meta-held)
        callback (:callback (meta prior))]
    (list 'quote [(identical? prior f) (identical? (meta prior) (meta f))
                  (identical? callback g) (identical? callback (:again (meta f)))
                  (= (hash callback) (hash g)) (callback)])))
