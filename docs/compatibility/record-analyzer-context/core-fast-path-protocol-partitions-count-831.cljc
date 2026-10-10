(def fast-path-protocol-partitions-count
  "total number of partitions"
  (core/let [c (count fast-path-protocols)
             m (core/mod c 32)]
    (if (core/zero? m)
      (core/quot c 32)
      (core/inc (core/quot c 32)))))
