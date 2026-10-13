;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn- tree-map-remove [comp tree k found]
  (if-not (nil? tree)
    (let [c (comp k (.-key tree))]
      (cond
        (zero? c)
        (do (aset found 0 tree)
            (tree-map-append (.-left tree) (.-right tree)))

        (neg? c)
        (let [del (tree-map-remove comp (.-left tree) k found)]
          (if (or (not (nil? del)) (not (nil? (aget found 0))))
            (if (instance? BlackNode (.-left tree))
              (balance-left-del (.-key tree) (.-val tree) del (.-right tree))
              (RedNode. (.-key tree) (.-val tree) del (.-right tree) nil))))

        :else
        (let [del (tree-map-remove comp (.-right tree) k found)]
          (if (or (not (nil? del)) (not (nil? (aget found 0))))
            (if (instance? BlackNode (.-right tree))
              (balance-right-del (.-key tree) (.-val tree) (.-left tree) del)
              (RedNode. (.-key tree) (.-val tree) (.-left tree) del nil))))))))
