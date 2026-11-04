;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn nthrest
  "Returns the nth rest of coll, coll when n is 0."
  [coll n]
    (if (implements? IDrop coll)
      (if (pos? n)
        (or (-drop coll (Math/ceil n)) ())
        coll)
      (loop [n n xs coll]
        (if-let [xs (and (pos? n) (seq xs))]
          (recur (dec n) (rest xs))
          xs))))
