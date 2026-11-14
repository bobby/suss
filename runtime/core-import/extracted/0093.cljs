;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn hash
  "Returns the hash code of its argument. Note this is the hash code
   consistent with =."
  [o]
  (cond
    (implements? IHash o)
    (bit-xor (-hash o) 0)

    (number? o)
    (if (js/isFinite o)
      (if-not (.isSafeInteger js/Number o)
        (hash-double o)
        (js-mod (Math/floor o) 2147483647))
      (case o
        ##Inf
        2146435072
        ##-Inf
        -1048576
        2146959360))

    ;; note: mirrors Clojure's behavior on the JVM, where the hashCode is
    ;; 1231 for true and 1237 for false
    ;; http://docs.oracle.com/javase/7/docs/api/java/lang/Boolean.html#hashCode%28%29
    (true? o) 1231

    (false? o) 1237

    (string? o)
    (m3-hash-int (hash-string o))

    (instance? js/Date o)
    (bit-xor (.valueOf o) 0)

    (nil? o) 0

    :else
    (bit-xor (-hash o) 0)))
