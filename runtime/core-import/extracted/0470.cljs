;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

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
