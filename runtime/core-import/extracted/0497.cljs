;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn ^boolean isa?
  "Returns true if (= child parent), or child is directly or indirectly derived from
  parent, either via a JavaScript type inheritance relationship or a
  relationship established via derive. h must be a hierarchy obtained
  from make-hierarchy, if not supplied defaults to the global
  hierarchy"
  ([child parent] (isa? @(get-global-hierarchy) child parent))
  ([h child parent]
   (or (= child parent)
       (and (js-fn? parent) (js-fn? child)
            (instance? parent child))
       (contains? ((:ancestors h) child) parent)
       (and (js-fn? child) (some #(contains? ((:ancestors h) %) parent) (supers child)))
       (and (vector? parent) (vector? child)
            (== (count parent) (count child))
            (loop [ret true i 0]
              (if (or (not ret) (== i (count parent)))
                ret
                (recur (isa? h (child i) (parent i)) (inc i))))))))
