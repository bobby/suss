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
      (assert (every? #(re-matches #"[a-f0-9]{64}" (second %)) (:macro-dependencies m)))
      (let [prior (edn/read-string (slurp (io/file "results/m12-decimal-conversions/aot-size-manifests" (.getName f))))]
        ;; Implementation fingerprints change with the repaired compiler.
        ;; All other data, including nil versus empty and every macro identity,
        ;; must survive the new encoding unchanged.
        (assert (= (dissoc prior :compiler-source-sha256 :wasm-sha256 :macro-dependencies)
                   (dissoc m :compiler-source-sha256 :wasm-sha256 :macro-dependencies)))
        (assert (= (nil? (:macro-dependencies prior)) (nil? (:macro-dependencies m))))
        (assert (= (mapv first (:macro-dependencies prior)) (mapv first (:macro-dependencies m))))))))
  (println "Clojure EDN read and print/read:" (count files) "actual emitted manifests"))
