;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn array-index-of [arr k]
  (cond
    (keyword? k) (array-index-of-keyword? arr k)

    (or (string? k) (number? k))
    (array-index-of-identical? arr k)

    (symbol? k) (array-index-of-symbol? arr k)

    (nil? k)
    (array-index-of-nil? arr)

    :else (array-index-of-equiv? arr k)))
