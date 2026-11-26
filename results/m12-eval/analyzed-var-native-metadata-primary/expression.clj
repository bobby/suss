(prn [(let [f (fn [] 42) v (eval (list 'def (with-meta 'catalog-function-metadata {:callback f}) 1))] [(identical? f (:callback (meta v))) ((:callback (meta v))) (identical? v (eval '(var catalog-function-metadata)))])
(let [x (with-meta [42] {:tag :kept}) v (eval (list 'def (with-meta 'catalog-immutable-metadata {:payload x :again x}) 1))] [(identical? x (:payload (meta v))) (identical? (:payload (meta v)) (:again (meta v))) (= x (:payload (meta v)))])
(let [f (fn [] 42) v (eval (list 'def (with-meta 'catalog-truthy-macro {:macro f}) 1))] [(.isMacro v) (identical? f (:macro (meta v)))])])
