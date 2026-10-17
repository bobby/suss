
          (def let-pattern-events (atom []))
          (let [outer 23
                __suss_destructure_vec_0 77
                [a b :as whole] (do (swap! let-pattern-events conj 1) [17 19])
                {:keys [x y] :or {x (do (swap! let-pattern-events conj 2) outer)
                                      y (do (swap! let-pattern-events conj 3) a)}}
                  (do (swap! let-pattern-events conj 4) {:y b})
                later (do (swap! let-pattern-events conj 5) (+ a x y))]
            [a b whole x y later __suss_destructure_vec_0 @let-pattern-events])
        