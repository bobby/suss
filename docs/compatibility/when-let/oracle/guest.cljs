
          (def when-let-events (atom []))
          [(when-let [x nil] x)
           (when-let [x false] x)
           (when-let [x 0] x)
           (when-let [x ""] x)
           (when-let [x []] x)
           (when-let [x {}] x)
           (when-let [x true])
           (when-let [x nil])
           (when-let [x (do (swap! when-let-events conj 1) 17)]
             (swap! when-let-events conj 10)
             (swap! when-let-events conj 20)
             x)
           (when-let [x (do (swap! when-let-events conj 2) false)]
             (swap! when-let-events conj 999))
           (let [x 31] (when-let [x (+ x 1)] x))
           (let [temp 77] (when-let [x 17] [temp x]))
           (when-let [[a b] [17 23]] [a b])
           (when-let [{:keys [a b] :or {b 23}} {:a 17}] [a b])
           (loop [n 3] (if (= n 0) 7 (when-let [x n] (recur (- x 1)))))
           @when-let-events]
        