;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn- tree-map-replace [comp tree k v]
  (let [tk (.-key tree)
        c  (comp k tk)]
    (cond (zero? c) (.replace tree tk v (.-left tree) (.-right tree))
          (neg? c)  (.replace tree tk (.-val tree) (tree-map-replace comp (.-left tree) k v) (.-right tree))
          :else     (.replace tree tk (.-val tree) (.-left tree) (tree-map-replace comp (.-right tree) k v)))))
