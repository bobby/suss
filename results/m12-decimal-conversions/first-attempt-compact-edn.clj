(require '[clojure.edn :as edn] '[clojure.java.io :as io])
(assert (= "1.12.6" (clojure-version)))
(let [files (filter #(.endsWith (.getName %) ".edn") (file-seq (io/file "results/m12-decimal-conversions/ci-repair/aot-size-after")))]
  (assert (seq files))
  (doseq [f files]
    (let [text (slurp f) m (edn/read-string text)]
      (assert (= 13 (count m)))
      (assert (= 2 (:format-version m)))
      (assert (= 12 (:runtime-abi m)))
      (assert (every? #(re-matches #"[a-f0-9]{64}" %) (keep m [:compiler-source-sha256 :source-sha256 :wasm-sha256])))
      (assert (= m (edn/read-string (pr-str m))))
      (when (seq (:macro-dependencies m))
        (assert (some #(= "macro:suss.core/with-precision" (first %)) (:macro-dependencies m)))
        (assert (every? #(re-matches #"[a-f0-9]{64}" (second %)) (:macro-dependencies m))))))
  (println "Clojure EDN read and print/read:" (count files) "actual emitted manifests"))
