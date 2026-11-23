;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn nat-int?
  "Return true if x satisfies int? and is a natural integer value."
  [x]
  (cond
    (integer? x)
    (not (neg? x))

    (instance? goog.math.Integer x)
    (not (.isNegative x))

    (instance? goog.math.Long x)
    (not (.isNegative x))

    :else false))
