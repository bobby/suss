(assert (= "1.12.6" (clojure-version)))
(do (declare review-meta-root) (let [v (eval '(defmacro review-meta-root [] 1))] [(.isMacro v) (with-redefs [review-meta-root (fn [& xs] 42)] [(.isMacro v) (eval '(review-meta-root))]) (.isMacro v) (try (eval '(review-meta-root)) (catch Throwable e :error))]))
