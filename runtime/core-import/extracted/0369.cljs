;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn- balance-left [key val ins right]
  (if (instance? RedNode ins)
    (cond
      (instance? RedNode (.-left ins))
      (RedNode. (.-key ins) (.-val ins)
              (.blacken (.-left ins))
              (BlackNode. key val (.-right ins) right nil)
              nil)

      (instance? RedNode (.-right ins))
      (RedNode. (.. ins -right -key) (.. ins -right -val)
                (BlackNode. (.-key ins) (.-val ins)
                            (.-left ins)
                            (.. ins -right -left)
                            nil)
                (BlackNode. key val
                            (.. ins -right -right)
                            right
                            nil)
                nil)

      :else
      (BlackNode. key val ins right nil))
    (BlackNode. key val ins right nil)))
