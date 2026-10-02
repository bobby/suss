;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn ranged-iterator [v start end]
  (let [i start]
    (RangedIterator. i (- i (js-mod i 32))
      (when (< start (count v))
        (unchecked-array-for v i))
      v start end)))
