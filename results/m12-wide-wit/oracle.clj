(assert (= "1.12.6" (clojure-version)))
(doseq [x [0 1 9223372036854775807 9223372036854775808N 18446744073709551615N]]
  (println (pr-str [(pr-str x) (pr-str (+' x 1)) (hash x)])))
(println (pr-str [(Double/doubleToRawLongBits (double 18446744073709551615N))
                  (Float/floatToRawIntBits (float 18446744073709551615N))
                  (Float/floatToRawIntBits (float 1/3))
                  (Double/doubleToRawLongBits (double 1/3))]))
