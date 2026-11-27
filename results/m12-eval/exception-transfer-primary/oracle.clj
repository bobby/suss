(assert (= "1.12.6" (clojure-version)))
(prn (let [v (eval '(defmacro review-exact-cause [] (throw (ex-info "outer" {:decimal 1.00M} (ex-info "inner" {:ratio 2/3}))))) ] (try (v nil nil) (catch Throwable e [(ex-message e) (ex-data e) (ex-message (ex-cause e)) (ex-data (ex-cause e))]))))
(prn (let [v (eval '(defmacro review-divide-error [] (/ 1 0)))] (try (v nil nil) (catch Throwable e [(ex-message e) (ex-data e)]))))
(prn (let [v (eval '(defmacro review-nil-message [] (throw (ex-info nil {}))))] (try (v nil nil) (catch Throwable e [(ex-message e) (ex-data e)]))))
