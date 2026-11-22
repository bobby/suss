(assert (= "1.12.6" (clojure-version)))
(let [s "[(== (double (float (identity 18014399583223809))) 1.8014398509481984E16) (== (double (unchecked-float (identity 18014399583223809))) 1.8014400656965632E16)]" r (pr-str (eval (read-string s)))] (prn s r) (assert (= "[true true]" r)))
(let [s "(let [x (loop [] 18014399583223809)] [(== (double (float x)) 1.8014398509481984E16) (== (double (unchecked-float x)) 1.8014400656965632E16)])" r (pr-str (eval (read-string s)))] (prn s r) (assert (= "[true true]" r)))
(let [s "(let [^long x (identity 18014399583223809)] [(== (double (float x)) 1.8014400656965632E16) (== (double (unchecked-float x)) 1.8014400656965632E16)])" r (pr-str (eval (read-string s)))] (prn s r) (assert (= "[true true]" r)))
