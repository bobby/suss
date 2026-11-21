(assert (= "1.12.6" (clojure-version)))
(prn 
          (mapv (fn [n]
            (let [before (into {} (map (fn [x] [x x])) (range n))
                  t (transient before)
                  edited (reduce (fn [t x] (assoc! t x (- x))) t (range n (+ n 70)))
                  after (persistent! edited)]
              [n (= before (into {} (map (fn [x] [x x])) (range n)))
               (= after (into before (map (fn [x] [x (- x)])) (range n (+ n 70))))
               (= after (persistent! (reduce conj! (transient before)
                    (mapv (fn [x] [x (- x)]) (range n (+ n 70))))))]))
            [0 1 7 8 15 16 31 32 63 64 255 256 1023 1024]))

          (def retained-map-values
            (let [a (atom 1) f (fn [] (swap! a inc))
                  t (transient (into {} (map (fn [x] [x x])) (range 64)))]
              (conj! t [1 a]) (assoc! t 2 f)
              (assoc! t nil false :tag nil "text" 1.5 9223372036854775807 :wide)
              [a f (persistent! t)]))
(System/gc)
(prn 
          (let [[a f p] retained-map-values events (atom []) t (transient p)
                _ (persistent! t)
                failed (try (assoc! (do (swap! events conj :receiver) t)
                                   (do (swap! events conj :key) 9)
                                   (do (swap! events conj :value) :bad))
                            :missing (catch Throwable e :threw))]
            [(identical? a (get p 1)) (identical? f (get p 2))
             ((get p 2)) @a (get p nil) (get p :tag :absent)
             (get p "text") (get p 9223372036854775807)
             failed @events (get p 9)]))
