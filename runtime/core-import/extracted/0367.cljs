;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn- compare-symbols [a b]
  (cond
   (identical? (.-str a) (.-str b)) 0
   (and (not (.-ns a)) (.-ns b)) -1
   (.-ns a) (if-not (.-ns b)
              1
              (let [nsc (garray/defaultCompare (.-ns a) (.-ns b))]
                (if (== 0 nsc)
                  (garray/defaultCompare (.-name a) (.-name b))
                  nsc)))
   :default (garray/defaultCompare (.-name a) (.-name b))))
