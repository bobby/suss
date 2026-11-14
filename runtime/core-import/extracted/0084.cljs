;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn- linear-traversal-nth
  ([coll n]
     (cond
       (nil? coll)     (throw (js/Error. "Index out of bounds"))
       (zero? n)       (if (seq coll)
                         (first coll)
                         (throw (js/Error. "Index out of bounds")))
       (indexed? coll) (-nth coll n)
       (seq coll)      (recur (next coll) (dec n))
       :else           (throw (js/Error. "Index out of bounds"))))
  ([coll n not-found]
     (cond
       (nil? coll)     not-found
       (zero? n)       (if (seq coll)
                         (first coll)
                         not-found)
       (indexed? coll) (-nth coll n not-found)
       (seq coll)      (recur (next coll) (dec n) not-found)
       :else           not-found)))
