(assert (= "1.12.6" (clojure-version)))
(println (clojure-version))

          (def retained-map-readers
            (mapv (fn [n]
              (let [ks (vec (map (fn [i] (keyword (str "copy" i))) (range n)))
                    original (with-meta (zipmap ks (range n)) {:stamp :old})
                    changed (reduce (fn [m i]
                              (assoc m (nth ks i) [i 9007199254740993])) original (range n))
                    final (assoc changed (nth ks 0) nil (nth ks (dec n)) false :new :added)]
                (fn []
                  (let [expected (vec (map (fn [i] [i 9007199254740993]) (range n)))]
                    [(count original) (count changed) (count final) (meta final)
                     (= (vec (map (fn [k] (get original k :missing)) ks)) (vec (range n)))
                     (= (vec (map (fn [k] (get changed k :missing)) ks)) expected)
                     (= (vec (map (fn [k] (get final k :missing)) ks))
                        (assoc expected 0 nil (dec n) false))
                     (get final :new) (map? final)])))) [1 9 37 129]))
(prn (mapv (fn [f] (f)) retained-map-readers))
