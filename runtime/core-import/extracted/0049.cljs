;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn hash-string [k]
  (when (> string-hash-cache-count 1024)
    (set! string-hash-cache (js-obj))
    (set! string-hash-cache-count 0))
  (if (nil? k)
    0
    (let [h (unchecked-get string-hash-cache k)]
      (if (number? h)
        h
        (add-to-string-hash-cache k)))))
