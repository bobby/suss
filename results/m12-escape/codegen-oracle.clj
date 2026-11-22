(assert (= "1.12.6" (clojure-version)))
(defn bound [] (loop [i 0 acc 0.0]
          (if (< i 1234591)
            (let [x (+ 3000000000 i) y (double i) f (fn [] (+ x y))]
              (recur (inc i) (unchecked-add acc (double (f))))) acc)))
(let [actual (bound)] (prn actual) (assert (= 3705297213702690.0 actual)))
