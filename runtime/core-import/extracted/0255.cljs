;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn into
  "Returns a new coll consisting of to-coll with all of the items of
  from-coll conjoined. A transducer may be supplied."
  ([] [])
  ([to] to)
  ([to from]
     (if-not (nil? to)
       (if (implements? IEditableCollection to)
         (-with-meta (persistent! (reduce -conj! (transient to) from)) (meta to))
         (reduce -conj to from))
       (reduce conj to from)))
  ([to xform from]
     (if (implements? IEditableCollection to)
       (let [tm (meta to)
             rf (fn
                  ([coll] (-> (persistent! coll) (-with-meta tm)))
                  ([coll v] (conj! coll v)))]
         (transduce xform rf (transient to) from))
       (transduce xform conj to from))))
