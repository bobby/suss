;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn count
  "Returns the number of items in the collection. (count nil) returns
  0.  Also works on strings, arrays, and Maps"
  [coll]
  (if-not (nil? coll)
    (cond
      (implements? ICounted coll)
      (-count coll)

      (array? coll)
      (alength coll)

      (string? coll)
      ^number (.-length coll)

      (implements? ISeqable coll)
      (accumulating-seq-count coll)

      :else (-count coll))
    0))
