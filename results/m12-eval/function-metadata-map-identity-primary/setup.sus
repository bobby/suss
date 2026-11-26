(defmacro identity-remember [f] (eval (list 'def 'identity-held f)) nil)
(defmacro identity-check [f]
  (let [prior (eval 'identity-held)]
    (list 'quote [(identical? prior f) (= prior f) (= (hash prior) (hash f))
                  (identical? (meta prior) (meta f)) (= (meta prior) (meta f))])))
