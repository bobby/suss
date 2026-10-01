;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn nth
  "Returns the value at the index. get returns nil if index out of
  bounds, nth throws an exception unless not-found is supplied.  nth
  also works for strings, arrays, regex Matchers and Lists, and,
  in O(n) time, for sequences."
  ([coll n]
    (cond
      (not (number? n))
      (throw (js/Error. "Index argument to nth must be a number"))

      (nil? coll)
      coll

      (implements? IIndexed coll)
      (-nth coll n)

      (array? coll)
      (if (and (< -1 n (.-length coll)))
        (aget coll (int n))
        (throw (js/Error. "Index out of bounds")))

      (string? coll)
      (if (and (< -1 n (.-length coll)))
        (.charAt coll (int n))
        (throw (js/Error. "Index out of bounds")))

      (or (implements? ISeq coll)
          (implements? ISequential coll))
      (if (neg? n)
        (throw (js/Error. "Index out of bounds"))
        (linear-traversal-nth coll n))

      (native-satisfies? IIndexed coll)
      (-nth coll n)

      :else
      (throw (js/Error. (str_ "nth not supported on this type "
                          (type->str (type coll)))))))
  ([coll n not-found]
    (cond
      (not (number? n))
      (throw (js/Error. "Index argument to nth must be a number."))

      (nil? coll)
      not-found

      (implements? IIndexed coll)
      (-nth coll n not-found)

      (array? coll)
      (if (and (< -1 n (.-length coll)))
        (aget coll (int n))
        not-found)

      (string? coll)
      (if (and (< -1 n (.-length coll)))
        (.charAt coll (int n))
        not-found)

      (or (implements? ISeq coll)
          (implements? ISequential coll))
      (if (neg? n)
        not-found
        (linear-traversal-nth coll n not-found))

      (native-satisfies? IIndexed coll)
      (-nth coll n not-found)

      :else
      (throw (js/Error. (str_ "nth not supported on this type "
                          (type->str (type coll))))))))
