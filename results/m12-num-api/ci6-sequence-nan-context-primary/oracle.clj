(println (pr-str [(= ##NaN ##NaN) (let [f =] (f ##NaN ##NaN)) ((identity =) ##NaN ##NaN) (let [x (identity ##NaN)] (= x x)) (= (list ##NaN) (list ##NaN))]))
