(assert (= "1.12.6" (clojure-version)))
(doseq [text ["(when-let 17 1)" "(when-let [] 1)" "(when-let [x] 1)" "(when-let [x 1 y 2] x)" "(when-let (x 1) 1)"]]
  (let [form (read-string text)
        root-message (fn [f] (try (f) :unexpected-success
                              (catch Throwable e (loop [e e] (if (ex-cause e) (recur (ex-cause e)) (ex-message e))))))]
    (println (pr-str (pr-str [(root-message #(macroexpand form))
                             (root-message #(apply @#'clojure.core/when-let form {} (rest form)))])))))
