(ns rand-object-proof)
(defn compute [^Object bound ^Object draw]
  (let [^Object product (* bound draw)] (int product)))
