(core/defmacro make-array
  ([size]
   (vary-meta
     (if (core/number? size)
       `(array ~@(take size (repeat nil)))
       `(js/Array. ~size))
     assoc :tag 'array))
  ([type size]
   `(cljs.core/make-array ~size))
  ([type size & more-sizes]
   (vary-meta
     `(let [dims#     (list ~@more-sizes)
            dimarray# (cljs.core/make-array ~size)]
        (dotimes [i# (alength dimarray#)]
          (aset dimarray# i# (apply cljs.core/make-array nil dims#)))
        dimarray#)
     assoc :tag 'array)))
