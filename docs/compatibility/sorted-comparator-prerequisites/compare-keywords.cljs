(defn- compare-keywords [a b]
  (cond
   (identical? (.-fqn a) (.-fqn b)) 0
   (and (not (.-ns a)) (.-ns b)) -1
   (.-ns a) (if-not (.-ns b)
              1
              (let [nsc (garray/defaultCompare (.-ns a) (.-ns b))]
                (if (== 0 nsc)
                  (garray/defaultCompare (.-name a) (.-name b))
                  nsc)))
   :default (garray/defaultCompare (.-name a) (.-name b))))
