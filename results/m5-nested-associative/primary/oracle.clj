(require '[clojure.data.json :as json])
(let [forms (json/read-str (slurp "inputs.json"))]
  (println (json/write-str
             (mapv (fn [source]
                     [source (pr-str (eval (read-string {:read-cond :allow} source)))]) forms))))
(binding [*out* *err*] (println (clojure-version)))
