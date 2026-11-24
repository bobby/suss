(println (pr-str [(let [[a] [##NaN]] [(= a a) (== a a) (identical? a a)]) (let [[a] [##NaN] b (+ a 0.0)] [(= a b) (== a b) (identical? a b)])]))
