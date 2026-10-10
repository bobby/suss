(core/defn- ->impl-map [impls]
  (core/loop [ret {} s impls]
    (if (seq s)
      (recur (assoc ret (first s) (take-while seq? (next s)))
        (drop-while seq? (next s)))
      ret)))
