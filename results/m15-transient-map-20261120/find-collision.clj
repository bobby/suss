(assert (= "1.12.6" (clojure-version)))
(loop [i -100000 seen {}]
 (if (> i 100000) (throw (ex-info "no collision" {}))
  (let [h (hash i)]
   (if-let [old (get seen h)] (prn [old i h])
    (recur (inc i) (assoc seen h i))))))
