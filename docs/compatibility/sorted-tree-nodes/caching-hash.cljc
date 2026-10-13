(core/defmacro caching-hash [coll hash-fn hash-key]
  (core/assert (clojure.core/symbol? hash-key) "hash-key is substituted twice")
  `(let [h# ~hash-key]
     (if-not (nil? h#)
       h#
       (let [h# (~hash-fn ~coll)]
         (set! ~hash-key h#)
         h#))))
