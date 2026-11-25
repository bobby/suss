(require (quote clojure.edn))
(let [a (clojure.edn/read-string (slurp "primary.stdout")) b (clojure.edn/read-string (slurp "native.stdout")) matches (mapv = a b)] (prn matches) (System/exit (if (every? true? matches) 0 1)))
