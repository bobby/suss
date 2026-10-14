(deftype Eduction [xform coll]
  Object
  (indexOf [coll x]
    (-indexOf coll x 0))
  (indexOf [coll x start]
    (-indexOf coll x start))
  (lastIndexOf [coll x]
    (-lastIndexOf coll x (count coll)))
  (lastIndexOf [coll x start]
    (-lastIndexOf coll x start))

  ISequential

  IIterable
  (-iterator [_]
    (.create TransformerIterator xform (iter coll)))

  ISeqable
  (-seq [_] (seq (sequence xform coll)))

  IReduce
  (-reduce [_ f] (transduce xform (completing f) coll))
  (-reduce [_ f init] (transduce xform (completing f) init coll))

  IPrintWithWriter
  (-pr-writer [coll writer opts]
    (pr-sequential-writer writer pr-writer "(" " " ")" opts coll)))
