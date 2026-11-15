;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn pr-sequential-writer [writer print-one begin sep end opts coll]
  (binding [*print-level* (when-not (nil? *print-level*) (dec *print-level*))]
    (if (and (not (nil? *print-level*)) (neg? *print-level*))
      (-write writer "#")
      (do
        (-write writer begin)
        (if (zero? (pr-opts-len opts))
          (when (seq coll)
            (-write writer (or (:more-marker opts) "...")))
          (do
            (when (seq coll)
              (print-one (first coll) writer opts))
            (loop [coll (next coll) n (dec (pr-opts-len opts))]
              (if (and coll (or (nil? n) (not (zero? n))))
                (do
                  (-write writer sep)
                  (print-one (first coll) writer opts)
                  (recur (next coll) (dec n)))
                (when (and (seq coll) (zero? n))
                  (-write writer sep)
                  (-write writer (or (:more-marker opts) "...")))))))
        (-write writer end)))))
