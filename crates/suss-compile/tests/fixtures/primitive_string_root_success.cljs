(def root-success-events [])
(def root-success-method (.-charCodeAt "AB"))
(let [selected (.charCodeAt
                 (do (set! root-success-events (conj root-success-events 1)) "AB")
                 (do (set! root-success-events (conj root-success-events 2))
                     (set! (.-auditMutationField root-success-method) 17)
                     (set! root-success-events (conj root-success-events 3))
                     1))]
  [selected
   (.-auditMutationField root-success-method)
   (identical? root-success-method (.-charCodeAt "AB"))
   (.call root-success-method "AB"
     (do (set! root-success-events (conj root-success-events 4)) 1))
   root-success-events])
