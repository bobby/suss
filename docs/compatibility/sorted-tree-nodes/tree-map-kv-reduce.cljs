(defn- tree-map-kv-reduce [node f init]
  (let [init (if-not (nil? (.-left node))
               (tree-map-kv-reduce (.-left node) f init)
               init)]
    (if (reduced? init)
      init
      (let [init (f init (.-key node) (.-val node))]
        (if (reduced? init)
          init
          (if-not (nil? (.-right node))
            (tree-map-kv-reduce (.-right node) f init)
            init))))))
