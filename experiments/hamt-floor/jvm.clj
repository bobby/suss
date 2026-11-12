;; The kernels of src/hamt.wat in JVM Clojure, on a PersistentHashMap of the
;; same 1024 keys: clojure -M jvm.clj [n] [reps]
(set! *warn-on-reflection* true)
(def m (zipmap (range 1024) (range 1024)))
(def ^"[Ljava.lang.Object;" sink (object-array 1))

(defn control ^long [^long n]
  (loop [i 0 c 0]
    (if (< i n)
      (let [k (bit-and i 1023)] (aset sink 0 (Long/valueOf k)) (recur (inc i) (if (nil? (aget sink 0)) c (inc c))))
      c)))

(defn get-loop ^long [^long n]
  (loop [i 0 c 0]
    (if (< i n)
      (recur (inc i) (if (nil? (get m (bit-and i 1023))) c (inc c)))
      c)))

(defn assoc-loop ^long [^long n]
  (loop [i 0]
    (if (< i n)
      (do (aset sink 0 (assoc m (bit-and i 1023) i)) (recur (inc i)))
      n)))

(let [[n reps] (map parse-long *command-line-args*)
      n (or n 10000000)
      reps (or reps 5)
      best (fn [f]
             (dotimes [_ 3] (f (quot n 10)))
             (apply min (for [_ (range reps)]
                          (let [t (System/nanoTime)]
                            (assert (= n (f n)))
                            (/ (double (- (System/nanoTime) t)) n)))))
      results (into {} (for [[k f] [["control" control] ["get" get-loop] ["assoc" assoc-loop]]] [k (best f)]))]
  (println (str "JVM Clojure " (clojure-version) ", Java " (System/getProperty "java.version") ", n=" n ", best of " reps))
  (println "\n| Kernel | ns per iteration | less control |\n| --- | --- | --- |")
  (doseq [k ["control" "get" "assoc"]]
    (println (format "| `%s` | %.2f | %.2f |" k (results k) (- (results k) (results "control"))))))
