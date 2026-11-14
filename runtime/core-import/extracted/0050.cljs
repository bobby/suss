;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn ^seq seq
  "Returns a seq on the collection. If the collection is
  empty, returns nil.  (seq nil) returns nil. seq also works on
  Strings."
  [coll]
  (when-not (nil? coll)
    (cond
      (implements? ISeqable coll)
      (-seq coll)

      (array? coll)
      (when-not (zero? (alength coll))
        (IndexedSeq. coll 0 nil))

      (string? coll)
      (when-not (zero? (.-length coll))
        (IndexedSeq. coll 0 nil))

      (js-iterable? coll)
      (es6-iterator-seq
        (.call (gobject/get coll ITER_SYMBOL) coll))

      (native-satisfies? ISeqable coll)
      (-seq coll)

      :else (throw (js/Error. (str_ coll " is not ISeqable"))))))
