;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn print-map [m print-one writer opts]
  (let [ns&lift-map (when (map? m)
                      (lift-ns m))
        ns (some-> ns&lift-map (aget 0))]
    (if ns
      (print-prefix-map (str_ "#:" ns) (aget ns&lift-map 1) print-one writer opts)
      (print-prefix-map nil m print-one writer opts))))
