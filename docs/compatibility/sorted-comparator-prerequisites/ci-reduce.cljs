(defn- ci-reduce
  "Accepts any collection which satisfies the ICount and IIndexed protocols and
reduces them without incurring seq initialization"
  ([^not-native cicoll f]
     (let [cnt (-count cicoll)]
       (if (zero? cnt)
         (f)
         (loop [val (-nth cicoll 0), n 1]
           (if (< n cnt)
             (let [nval (f val (-nth cicoll n))]
               (if (reduced? nval)
                 @nval
                 (recur nval (inc n))))
             val)))))
  ([^not-native cicoll f val]
     (let [cnt (-count cicoll)]
       (loop [val val, n 0]
         (if (< n cnt)
           (let [nval (f val (-nth cicoll n))]
             (if (reduced? nval)
               @nval
               (recur nval (inc n))))
           val)))))
