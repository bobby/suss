;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn ^number parse-long
  "Parse string of decimal digits with optional leading -/+ and return an
  integer value, or nil if parse fails"
  [s]
  (if (string? s)
    (and (re-matches #"[+-]?\d+" s)
         (let [i (js/parseInt s)]
           (when (and (<= i js/Number.MAX_SAFE_INTEGER)
                      (>= i js/Number.MIN_SAFE_INTEGER))
             i)))
    (throw (js/Error. (parsing-err s)))))
