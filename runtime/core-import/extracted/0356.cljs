;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(deftype RecordIter [^:mutable i record base-count fields ext-map-iter]
  Object
  (hasNext [_]
    (or (< i base-count) (.hasNext ext-map-iter)))
  (next [_]
    (if (< i base-count)
      (let [k (nth fields i)]
        (set! i (inc i))
        (MapEntry. k (-lookup record k) nil))
      (.next ext-map-iter)))
  (remove [_] (js/Error. "Unsupported operation")))
