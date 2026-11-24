(println (pr-str [(= ##NaN ##NaN) (== ##NaN ##NaN) (let [a (+ ##NaN 0.0) b (+ ##NaN 0.0)] (= a b)) (let [a (identity ##NaN)] (= a a)) (= [##NaN] [##NaN])]))
