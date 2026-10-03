;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn vec
  "Creates a new vector containing the contents of coll. JavaScript arrays
  will be aliased and should not be modified."
  [coll]
  (cond
    (map-entry? coll)
    [(key coll) (val coll)]

    (vector? coll)
    (with-meta coll nil)

    (array? coll)
    (.fromArray PersistentVector coll true)

    :else
    (-persistent!
      (reduce -conj!
        (-as-transient (.-EMPTY PersistentVector))
        coll))))
