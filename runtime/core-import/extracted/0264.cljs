;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn set
  "Returns a set of the distinct elements of coll."
  [coll]
  (if (set? coll)
    (with-meta coll nil)
    (let [in (seq coll)]
      (cond
        (nil? in) #{}

        (and (instance? IndexedSeq in) (zero? (.-i in)))
        (.createAsIfByAssoc PersistentHashSet (.-arr in))

        :else
        (loop [^not-native in  in
               ^not-native out (-as-transient #{})]
          (if-not (nil? in)
            (recur (next in) (-conj! out (-first in)))
            (persistent! out)))))))
