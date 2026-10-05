;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn- str_
  "Implementation detail. Internal str without circularity on IndexedSeq.
  @param x
  @param {...*} var_args"
  [x var-args]
  (cond
    ;; works whether x is undefined or null (cljs nil)
    (nil? x) ""
    ;; if we have no more parameters, return
    (undefined? var-args) (.join #js [x] "")
    ;; var arg case without relying on CLJS fn machinery which creates
    ;; a circularity via IndexedSeq
    :else
    (let [sb   (StringBuffer.)
          args (js-arguments)
          len  (alength args)]
      (loop [i 0]
        (if (< i len)
          (do
            (.append sb (cljs.core/str_ (aget args i)))
            (recur (inc i)))
          (.toString sb))))))
