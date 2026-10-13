(defn rsubseq
  "sc must be a sorted collection, test(s) one of <, <=, > or
  >=. Returns a reverse seq of those entries with keys ek for
  which (test (.. sc comparator (compare ek key)) 0) is true"
  ([sc test key]
     (let [include (mk-bound-fn sc test key)]
       (if (#{< <=} test)
         (when-let [[e :as s] (-sorted-seq-from sc key false)]
           (if (include e) s (next s)))
         (take-while include (-sorted-seq sc false)))))
  ([sc start-test start-key end-test end-key]
     (when-let [[e :as s] (-sorted-seq-from sc end-key false)]
       (take-while (mk-bound-fn sc start-test start-key)
                   (if ((mk-bound-fn sc end-test end-key) e) s (next s))))))
