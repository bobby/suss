(assert (= "1.12.6" (clojure-version)))
(println (clojure-version))
(prn 
          [(reduce + 0 (chunk-cons (clojure.lang.ArrayChunk. (object-array [9007199254740992 1])) nil))
           (reduce + 0 (chunk-cons (clojure.lang.ArrayChunk. (object-array [1 2.5 3])) nil))
           (reduce + 7 (chunk-cons (clojure.lang.ArrayChunk. (object-array [99 -3 4 99]) 1 3) nil))
           (try (reduce + 0 (chunk-cons
                  (clojure.lang.ArrayChunk. (object-array [9223372036854775807 1 0.5])) nil))
                :missing (catch Throwable e (ex-message e)))
           (try (reduce + 0 (chunk-cons
                  (clojure.lang.ArrayChunk. (object-array [-9223372036854775808 -1 0.5])) nil))
                :missing (catch Throwable e (ex-message e)))
           (try (reduce + 0 (chunk-cons (clojure.lang.ArrayChunk. (object-array [1 nil])) nil))
                :missing (catch Throwable e :threw))])
(prn 
          [(let [events (atom []) a (object-array [1 2 3])]
             [(reduce (fn [acc x] (swap! events conj x)
                        (when (= x 1) (aset a 1 20)) (+ acc x))
                      0 (chunk-cons (clojure.lang.ArrayChunk. a) nil)) @events])
           (let [events (atom []) more (lazy-seq (swap! events conj :tail) (list 9))]
             [(reduce (fn [acc x] (swap! events conj x)
                        (if (= x 2) (reduced acc) (+ acc x)))
                      0 (chunk-cons (clojure.lang.ArrayChunk. (object-array [1 2 3])) more)) @events])
           (let [events (atom []) original +]
             (with-redefs [+ (fn [a b] (swap! events conj b) (original a b))]
               [(reduce + 0 (chunk-cons (clojure.lang.ArrayChunk. (object-array [1 2 3])) nil))
                @events]))])
