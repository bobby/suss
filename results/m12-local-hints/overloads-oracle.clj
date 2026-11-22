(assert (= "1.12.6" (clojure-version)))
(doseq [[klass names] [[clojure.lang.Numbers #{"and" "or" "xor" "andNot" "not" "shiftLeft" "shiftRight" "unsignedShiftRight" "quotient" "remainder" "max" "min" "add" "minus" "multiply" "divide" "unchecked_add" "unchecked_inc"}]
                       [clojure.lang.Util #{"equiv"}]]
        method (sort-by str (.getMethods klass))
        :when (contains? names (.getName method))]
  (prn (str klass) (.getName method) (mapv #(.getName %) (.getParameterTypes method)) (.getName (.getReturnType method))))
(doseq [source ["(let [^double x (identity 5)] (bit-and x 1))"
                "(let [^long x (identity 5.5)] (bit-and x (identity 1)))"
                "(let [^long x (identity 5.5) ^double y (identity 1)] (bit-and x y))"
                "(let [^long x (identity 5.5)] (bit-shift-left x (identity 1)))"]]
  (prn source (try [:value (eval (read-string source))] (catch Throwable e [:error (ex-message e) (some-> e ex-cause ex-message)]))))
