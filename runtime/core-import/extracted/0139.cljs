;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(deftype RangedIterator [^:mutable i ^:mutable base ^:mutable arr v start end]
  Object
  (hasNext [this]
    (< i end))
  (next [this]
    (when (== (- i base) 32)
      (set! arr (unchecked-array-for v i))
      (set! base (+ base 32)))
    (let [ret (aget arr (bit-and i 0x01f))]
      (set! i (inc i))
      ret)))
