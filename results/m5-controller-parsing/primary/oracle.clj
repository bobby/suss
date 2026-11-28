(require '[clojure.edn :as edn])
(assert (= "1.12.6" (clojure-version)))
(prn {:clojure-version (clojure-version)})
(doseq [s (edn/read-string (slurp "inputs.edn"))]
  (prn [(parse-long s) (try (Long/parseLong s) (catch NumberFormatException _ :error))]))
